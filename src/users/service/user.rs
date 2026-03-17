use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use password_hash::SaltString;
use rand::thread_rng;
use sqlx::Row;

use crate::{
    config::AppState,
    errors::CustomError,
    private::{APP_ID, APP_SECRET, TOKEN_SECRET_KEY},
    users::models::user::{
        GenderEnum, IsRegisterResponse, LoginInput, LoginMethodEnum, LoginResponse,
        ProfileUpdateInput, RegisterInput, RoleSwitchInput, RoleSwitchResult, UserInfoResponse,
        UserPublic, UserRecord, UserRoleEnum, UserTokenClaims,
    },
    utils::{validate_nickname, validate_username},
};

pub struct UserService;

impl UserService {
    pub fn hash_password(plain: &str) -> Result<(String, String), String> {
        let salt = SaltString::generate(&mut thread_rng());
        let argon = Argon2::default();
        let hash = argon
            .hash_password(plain.as_bytes(), &salt)
            .map_err(|e| e.to_string())?
            .to_string();
        Ok((hash, "argon2id".to_string()))
    }

    pub fn verify_password(plain: &str, stored_hash: &str) -> Result<bool, String> {
        let parsed = PasswordHash::new(stored_hash).map_err(|e| e.to_string())?;
        let argon = Argon2::default();
        Ok(argon.verify_password(plain.as_bytes(), &parsed).is_ok())
    }

    // 微信登录
    pub async fn weixin_login(code: &str) -> Result<String, CustomError> {
        let res = reqwest::get(
            "https://api.weixin.qq.com/sns/jscode2session?grant_type=authorization_code&appid="
                .to_string()
                + APP_ID
                + "&secret="
                + APP_SECRET
                + "&js_code="
                + code,
        )
        .await?
        .text()
        .await?;

        let response_json: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_str(&res);
        match response_json {
            Ok(obj) => {
                let mut openid = "";
                if let Some(val) = obj.get("openid") {
                    if let Some(t) = val.as_str() {
                        openid = t;
                    }
                }
                Ok(openid.to_string())
            }
            Err(_) => Err(CustomError::BadRequest("openid 获取失败".to_string())),
        }
    }

    pub async fn register(data: RegisterInput, state: &AppState) -> Result<(), CustomError> {
        let db_pool = &state.db_pool;
        if data.username.is_empty() || data.password.is_empty() {
            return Err(CustomError::BadRequest("缺少账号或密码".into()));
        }

        validate_username(&data.username).map_err(|e| CustomError::BadRequest(e.to_string()))?;

        // 检查是否已存在
        let exists_row = sqlx::query("SELECT COUNT(*) FROM users WHERE username = $1")
            .bind(&data.username)
            .fetch_one(db_pool)
            .await?;
        let exists: i64 = exists_row.get(0);
        if exists > 0 {
            return Err(CustomError::BadRequest("账号已存在".into()));
        }

        let (pwd_hash, algo) =
            Self::hash_password(&data.password).map_err(|e| CustomError::internal(e))?;

        sqlx::query(
        r#"INSERT INTO users (
            username, nick_name, open_id, password_hash, password_algo, gender, birthday, username_change, login_method, role, love_point, status, is_temp_password
        ) VALUES (
            $1, $2, $3, $4, $5, $6, NULL, FALSE, $7, $8, 0, 1, FALSE
        )"#
    )
    .bind(&data.username)
    .bind(&data.username)
    .bind(&data.open_id)
    .bind(&pwd_hash)
    .bind(&algo)
    .bind(GenderEnum::UNKNOWN)
    .bind(LoginMethodEnum::PASSWORD)
    .bind(UserRoleEnum::ORDERING)
    .execute(db_pool).await?;

        Ok(())
    }

    pub async fn login(user: LoginInput, state: &AppState) -> Result<LoginResponse, CustomError> {
        let db_pool = &state.db_pool;
        let mut account = user.username.clone();

        if let Some(code) = &user.weixin_code {
            account = Self::weixin_login(&code).await?;
        };

        let record = sqlx::query_as::<_, UserRecord>(
        r#"SELECT u.user_id, u.username, u.nick_name, u.email, u.role, u.love_point, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
           (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
           FROM users u WHERE u.username = $1 OR u.open_id = $1"#
    )
    .bind(&account)
    .fetch_optional(db_pool)
    .await?;

        let record = match record {
            Some(r) => r,
            None => {
                if user.weixin_code.is_some() {
                    return Err(CustomError::NotFound(account));
                } else {
                    return Err(CustomError::BadRequest("账号不存在".into()));
                }
            }
        };

        let stored = record.password_hash.clone().unwrap_or_default();
        if let Some(ref pwd) = user.password {
            if !Self::verify_password(pwd, &stored).unwrap_or(false) {
                return Err(CustomError::BadRequest("账号或密码错误".into()));
            }
        } else if user.weixin_code.is_some() {
            // 微信登录且用户存在，允许登录
        } else {
            return Err(CustomError::BadRequest("缺少密码".into()));
        }

        sqlx::query("UPDATE users SET last_login_at = $2, login_method = $3 WHERE user_id = $1")
            .bind(record.user_id)
            .bind(Utc::now())
            .bind(LoginMethodEnum::PASSWORD)
            .execute(db_pool)
            .await?;

        let public: UserPublic = record.clone().into();
        let exp = chrono::Local::now().timestamp() + 3600 * 24 * 7;
        let claims = UserTokenClaims {
            user_id: record.user_id,
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TOKEN_SECRET_KEY),
        )
        .map_err(|e| CustomError::internal(e.to_string()))?;

        let _ = state.redis_cache.set_user_public(&public, 3600).await;

        Ok(LoginResponse {
            token,
            user: public,
        })
    }

    pub async fn get_current_info(
        user_id: i64,
        state: &AppState,
    ) -> Result<UserPublic, CustomError> {
        let db = &state.db_pool;
        let rec = sqlx::query_as::<_, UserRecord>(r#"
        SELECT u.user_id, u.username, u.email, u.nick_name, u.role, u.love_point, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
        FROM users u WHERE u.user_id = $1
    "#)
    .bind(user_id)
    .fetch_one(db)
    .await?;
        Ok(rec.into())
    }

    pub async fn get_user_info(
        username: &str,
        state: &AppState,
    ) -> Result<UserInfoResponse, CustomError> {
        let db = &state.db_pool;
        let rec = sqlx::query_as::<_, UserRecord>(r#"
        SELECT u.user_id, u.username, u.nick_name, u.email, u.role, u.love_point, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
        FROM users u WHERE u.username = $1 OR u.open_id = $1
    "#)
    .bind(username)
    .fetch_optional(db).await?;
        Ok(UserInfoResponse {
            exists: rec.is_some(),
            user: rec.map(|r| r.into()),
        })
    }

    pub async fn is_register(
        username: &str,
        state: &AppState,
    ) -> Result<IsRegisterResponse, CustomError> {
        if username.trim().is_empty() {
            return Err(CustomError::BadRequest("用户名不能为空".into()));
        }
        let db = &state.db_pool;
        let exists = sqlx::query_scalar::<_, i64>("SELECT user_id FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(db)
            .await?;
        Ok(IsRegisterResponse {
            registered: exists.is_some(),
        })
    }

    pub async fn change_info(
        data: ProfileUpdateInput,
        state: &AppState,
    ) -> Result<UserPublic, CustomError> {
        let db_pool = &state.db_pool;
        let mut current_username = data.username.clone();

        let rec = sqlx::query_as::<_, UserRecord>(r#"
        SELECT u.user_id, u.username, u.email, u.nick_name, u.role, u.love_point, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at,
               u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at,
               u.is_temp_password, u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
        FROM users u WHERE u.username = $1
    "#)
    .bind(&data.username)
    .fetch_optional(db_pool)
    .await?;

        let rec = match rec {
            Some(r) => r,
            None => return Err(CustomError::BadRequest("账号不存在".into())),
        };

        if let Some(new_username) = &data.new_username {
            validate_username(new_username).map_err(|e| CustomError::BadRequest(e.to_string()))?;
            let exists_row = sqlx::query("SELECT COUNT(*) FROM users WHERE username = $1")
                .bind(new_username)
                .fetch_optional(db_pool)
                .await?;
            let exists: i64 = exists_row.unwrap().get(0);
            if exists > 0 {
                return Err(CustomError::BadRequest("用户名已存在".into()));
            }
            sqlx::query(
                "UPDATE users SET username = $2, username_change = TRUE WHERE username = $1",
            )
            .bind(&current_username)
            .bind(new_username)
            .execute(db_pool)
            .await?;
            current_username = new_username.clone();
        } else if data.avatar.is_some()
            || data.gender.is_some()
            || data.birthday.is_some()
            || data.nick_name.is_some()
        {
            if let Some(nick) = &data.nick_name {
                validate_nickname(nick).map_err(|e| CustomError::BadRequest(e.to_string()))?;
            }
            sqlx::query("UPDATE users SET avatar = COALESCE($2, avatar), gender = COALESCE($3, gender), birthday = COALESCE($4, birthday), nick_name = COALESCE($5, nick_name) WHERE username = $1")
            .bind(&current_username)
            .bind(&data.avatar)
            .bind(&data.gender)
            .bind(&data.birthday)
            .bind(&data.nick_name)
            .execute(db_pool)
            .await?;
        }

        if let Some(new_pwd) = &data.new_password {
            if let Some(old_pwd) = &data.old_password {
                if let Some(stored) = &rec.password_hash {
                    if !Self::verify_password(old_pwd, stored).unwrap_or(false) {
                        return Err(CustomError::BadRequest("旧密码错误".into()));
                    }
                } else {
                    return Err(CustomError::BadRequest("无旧密码记录".into()));
                }
            }
            let (hash, algo) =
                Self::hash_password(new_pwd).map_err(|e| CustomError::internal(e))?;
            sqlx::query("UPDATE users SET password_hash = $2, password_algo = $3, password_updated_at = $4, is_temp_password = FALSE WHERE username = $1")
            .bind(&current_username)
            .bind(&hash)
            .bind(&algo)
            .bind(Utc::now())
            .execute(db_pool)
            .await?;
        }

        let updated = sqlx::query_as::<_, UserRecord>(r#"
        SELECT u.user_id, u.username, u.email, u.nick_name, u.role, u.love_point, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at,
               u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at,
               u.is_temp_password, u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
        FROM users u WHERE u.username = $1
    "#)
    .bind(&current_username)
    .fetch_one(db_pool)
    .await?;

        Ok(updated.into())
    }

    pub async fn switch_role(
        user_id: i64,
        body: RoleSwitchInput,
        state: &AppState,
    ) -> Result<RoleSwitchResult, CustomError> {
        let db = &state.db_pool;
        let group_row_opt = if let Some(gid) = body.group_id {
            sqlx::query("SELECT group_id, group_type::TEXT AS group_type FROM association_groups WHERE group_id=$1")
            .bind(gid)
            .fetch_optional(db)
            .await?
        } else {
            sqlx::query("SELECT g.group_id, g.group_type::TEXT AS group_type FROM association_groups g JOIN association_group_members m ON g.group_id=m.group_id WHERE m.user_id=$1 AND g.group_type='PAIR' LIMIT 1")
            .bind(user_id)
            .fetch_optional(db).await?
        };
        let Some(group_row) = group_row_opt else {
            return Err(CustomError::BadRequest("未找到可用的PAIR关联组".into()));
        };
        let group_id: i64 = group_row.get("group_id");
        let gtype: String = group_row
            .try_get::<String, _>("group_type")
            .unwrap_or_else(|_| "".into());
        if gtype != "PAIR" {
            return Err(CustomError::BadRequest("仅支持PAIR类型组内角色互换".into()));
        }

        let members = sqlx::query("SELECT user_id, role_in_group::TEXT AS role_in_group FROM association_group_members WHERE group_id=$1 ORDER BY user_id")
        .bind(group_id)
        .fetch_all(db).await?;
        if members.len() != 2 {
            return Err(CustomError::BadRequest("组成员数量必须为2".into()));
        }

        let mut self_role: Option<String> = None;
        let mut counterpart: Option<(i64, String)> = None;
        for r in &members {
            let uid: i64 = r.get("user_id");
            let role: String = r.get("role_in_group");
            if uid == user_id {
                self_role = Some(role);
            } else {
                counterpart = Some((uid, role));
            }
        }
        let Some(self_role_str) = self_role else {
            return Err(CustomError::BadRequest("当前用户不在该组".into()));
        };
        let Some((cp_uid, cp_role_str)) = counterpart else {
            return Err(CustomError::BadRequest("未找到对方成员".into()));
        };

        if self_role_str == cp_role_str {
            return Err(CustomError::BadRequest("双方角色相同，无法互换".into()));
        }
        if !(self_role_str == "ORDERING" || self_role_str == "RECEIVING") {
            return Err(CustomError::BadRequest("当前角色不支持互换".into()));
        }

        let incomplete_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orders WHERE group_id=$1 AND status IN ('PENDING_ACCEPT', 'IN_PROGRESS', 'BREEDER_FINISHED', 'CONFIRMED_UNFINISHED')",
        )
        .bind(group_id)
        .fetch_one(db)
        .await?;

        if incomplete_count > 0 {
            return Err(CustomError::BadRequest(
                "当前组内存在未完成订单，无法切换角色".into(),
            ));
        }

        let new_self_role = if self_role_str == "ORDERING" {
            "RECEIVING"
        } else {
            "ORDERING"
        };
        let new_cp_role = if cp_role_str == "ORDERING" {
            "RECEIVING"
        } else {
            "ORDERING"
        };

        let mut tx = db.begin().await?;
        sqlx::query(
        "UPDATE association_group_members SET role_in_group=$3::group_member_role_enum WHERE group_id=$1 AND user_id=$2",
    )
    .bind(group_id)
    .bind(user_id)
    .bind(&new_self_role)
    .execute(&mut *tx)
    .await?;
        sqlx::query(
        "UPDATE association_group_members SET role_in_group=$3::group_member_role_enum WHERE group_id=$1 AND user_id=$2",
    )
    .bind(group_id)
    .bind(cp_uid)
    .bind(&new_cp_role)
    .execute(&mut *tx)
    .await?;
        sqlx::query(
        "UPDATE users SET role=$2::user_role_enum, last_role_switch_at=NOW(), updated_at=NOW() WHERE user_id=$1",
    )
    .bind(user_id)
    .bind(&new_self_role)
    .execute(&mut *tx)
    .await?;
        sqlx::query(
        "UPDATE users SET role=$2::user_role_enum, last_role_switch_at=NOW(), updated_at=NOW() WHERE user_id=$1",
    )
    .bind(cp_uid)
    .bind(&new_cp_role)
    .execute(&mut *tx)
    .await?;

        tx.commit().await?;

        let switched_at = Utc::now();
        let result = RoleSwitchResult {
            group_id,
            switched_at,
            user_id,
            new_role: if new_self_role == "ORDERING" {
                UserRoleEnum::ORDERING
            } else {
                UserRoleEnum::RECEIVING
            },
            counterpart_user_id: cp_uid,
            counterpart_new_role: if new_cp_role == "ORDERING" {
                UserRoleEnum::ORDERING
            } else {
                UserRoleEnum::RECEIVING
            },
        };
        Ok(result)
    }
}
