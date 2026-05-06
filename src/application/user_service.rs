// 应用服务层 - 用户服务
// 包含用户登录、注册、信息管理、组服务等业务用例

use std::sync::Arc;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use password_hash::SaltString;
use rand::thread_rng;
use sqlx::{PgPool, Row};
use crate::config::{AppState, TOKEN_SECRET_KEY};
use crate::domain::user::{
    LoginInput, RegisterInput, UserPublic, ProfileUpdateInput, IsRegisterQuery, IsRegisterResponse, LoginResponse, UserInfoResponse, RoleSwitchInput, RoleSwitchResult,
    InvitationListOut, NewInvitationInput, ConfirmInvitationInput, InvitationRequestOut,
    UnbindRequestInput, GroupInfoOut, BindUserDirectlyInput, GroupUpdateInput,
    GroupPointConfig, GroupPointConfigUpdateInput, UserRole, Gender, LoginMethod,
};
use crate::domain::user::entities::UserRecord;
use crate::errors::CustomError;

/// 用户应用服务
pub struct UserService;

impl UserService {
    /// 注册
    pub async fn register(
        input: RegisterInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        let db_pool = &state.db_pool;
        if input.username.is_empty() || input.password.is_empty() {
            return Err(CustomError::BadRequest("缺少账号或密码".into()));
        }

        // 检查是否已存在
        let exists_row = sqlx::query("SELECT COUNT(*) FROM users WHERE username = $1")
            .bind(&input.username)
            .fetch_one(db_pool)
            .await?;
        let exists: i64 = exists_row.get(0);
        if exists > 0 {
            return Err(CustomError::BadRequest("账号已存在".into()));
        }

        let (pwd_hash, algo) = Self::hash_password(&input.password)
            .map_err(|e| CustomError::internal(e))?;

        sqlx::query(
            r#"INSERT INTO users (
                username, nick_name, open_id, password_hash, password_algo, gender, birthday, username_change, login_method, role, love_point, status, is_temp_password
            ) VALUES (
                $1, $2, $3, $4, $5, $6, NULL, FALSE, $7, $8, 0, 1, FALSE
            )"#
        )
        .bind(&input.username)
        .bind(&input.username)
        .bind(&input.open_id)
        .bind(&pwd_hash)
        .bind(&algo)
        .bind(Gender::Unknown)
        .bind(LoginMethod::Password)
        .bind(UserRole::Ordering)
        .execute(db_pool)
        .await?;

        Ok(())
    }

    /// 登录
    pub async fn login(
        input: LoginInput,
        state: &Arc<AppState>,
    ) -> Result<LoginResponse, CustomError> {
        let db_pool = &state.db_pool;
        let mut account = input.username.clone();

        // 微信登录
        if let Some(code) = &input.weixin_code {
            account = Self::weixin_login(code).await?;
        };

        let record = sqlx::query_as::<_, UserRecord>(
            r#"SELECT u.user_id, u.username, u.nick_name, u.email, u.role, u.love_point, u.diamond, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
               FROM users u WHERE u.username = $1 OR u.open_id = $1"#
        )
        .bind(&account)
        .fetch_optional(db_pool)
        .await?;

        let record = match record {
            Some(r) => r,
            None => {
                if input.weixin_code.is_some() {
                    return Err(CustomError::NotFound(account));
                } else {
                    return Err(CustomError::BadRequest("账号不存在".into()));
                }
            }
        };

        // 验证密码
        if let Some(ref pwd) = input.password {
            let stored = record.password_hash.clone().unwrap_or_default();
            if !Self::verify_password(pwd, &stored).unwrap_or(false) {
                return Err(CustomError::BadRequest("账号或密码错误".into()));
            }
        } else if input.weixin_code.is_none() {
            return Err(CustomError::BadRequest("缺少密码".into()));
        }

        // 更新最后登录时间
        sqlx::query("UPDATE users SET last_login_at = $2, login_method = $3 WHERE user_id = $1")
            .bind(record.user_id)
            .bind(Utc::now())
            .bind(LoginMethod::Password)
            .execute(db_pool)
            .await?;

        let public: UserPublic = UserPublic::from(record.clone());
        let exp = chrono::Local::now().timestamp() + 3600 * 24 * 7;

        #[derive(serde::Serialize)]
        struct UserTokenClaims {
            exp: i64,
            user_id: i64,
        }

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

    /// 获取当前用户信息
    pub async fn get_current_info(
        user_id: i64,
        state: &Arc<AppState>,
    ) -> Result<UserPublic, CustomError> {
        let db = &state.db_pool;
        let rec = sqlx::query_as::<_, UserRecord>(r#"
            SELECT u.user_id, u.username, u.email, u.nick_name, u.role, u.love_point, u.diamond, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
                   (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
            FROM users u WHERE u.user_id = $1
        "#)
        .bind(user_id)
        .fetch_one(db)
        .await?;
        Ok(UserPublic::from(rec))
    }

    /// 根据用户名获取用户信息
    pub async fn get_user_info(
        username: &str,
        state: &Arc<AppState>,
    ) -> Result<UserInfoResponse, CustomError> {
        let db = &state.db_pool;
        let rec = sqlx::query_as::<_, UserRecord>(r#"
            SELECT u.user_id, u.username, u.email, u.nick_name, u.role, u.love_point, u.diamond, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
                   (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
            FROM users u WHERE u.username = $1
        "#)
        .bind(username)
        .fetch_optional(db)
        .await?;

        match rec {
            Some(r) => Ok(UserInfoResponse {
                is_register: true,
                user: Some(UserPublic::from(r)),
            }),
            None => Ok(UserInfoResponse {
                is_register: false,
                user: None,
            }),
        }
    }

    /// 判断用户名是否已注册
    pub async fn is_register(
        username: &str,
        state: &Arc<AppState>,
    ) -> Result<IsRegisterResponse, CustomError> {
        let db = &state.db_pool;
        let exists_row = sqlx::query("SELECT COUNT(*) FROM users WHERE username = $1")
            .bind(username)
            .fetch_one(db)
            .await?;
        let exists: i64 = exists_row.get(0);
        Ok(IsRegisterResponse {
            is_register: exists > 0,
        })
    }

    /// 修改用户信息
    pub async fn change_info(
        input: ProfileUpdateInput,
        state: &Arc<AppState>,
    ) -> Result<UserPublic, CustomError> {
        todo!("迁移修改用户信息")
    }

    /// 切换角色
    pub async fn switch_role(
        user_id: i64,
        input: RoleSwitchInput,
        state: &Arc<AppState>,
    ) -> Result<RoleSwitchResult, CustomError> {
        todo!("迁移切换角色")
    }

    // ========== 辅助方法 ==========

    fn hash_password(plain: &str) -> Result<(String, String), String> {
        let salt = SaltString::generate(&mut thread_rng());
        let argon = Argon2::default();
        let hash = argon
            .hash_password(plain.as_bytes(), &salt)
            .map_err(|e| e.to_string())?
            .to_string();
        Ok((hash, "argon2id".to_string()))
    }

    fn verify_password(plain: &str, stored_hash: &str) -> Result<bool, String> {
        let parsed = PasswordHash::new(stored_hash).map_err(|e| e.to_string())?;
        let argon = Argon2::default();
        Ok(argon.verify_password(plain.as_bytes(), &parsed).is_ok())
    }

    async fn weixin_login(code: &str) -> Result<String, CustomError> {
        use crate::private::{APP_ID, APP_SECRET};

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
}

/// 组服务
pub struct GroupService;

impl GroupService {
    /// 获取邀请列表
    pub async fn get_invitation(
        user_id: i64,
        state: &Arc<AppState>,
    ) -> Result<InvitationListOut, CustomError> {
        todo!("迁移获取邀请列表")
    }

    /// 创建邀请
    pub async fn new_invitation(
        user_id: i64,
        input: NewInvitationInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        todo!("迁移创建邀请")
    }

    /// 确认邀请
    pub async fn confirm_invitation(
        user_id: i64,
        invitation_id: i64,
        input: ConfirmInvitationInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        todo!("迁移确认邀请")
    }

    /// 取消邀请
    pub async fn cancel_invitation(
        invitation_id: i64,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        todo!("迁移取消邀请")
    }

    /// 解绑请求
    pub async fn unbind_request(
        user_id: i64,
        input: UnbindRequestInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        todo!("迁移解绑请求")
    }

    /// 获取群组信息
    pub async fn get_group_info(
        group_id: i64,
        state: &Arc<AppState>,
    ) -> Result<GroupInfoOut, CustomError> {
        let db = &state.db_pool;

        // 获取群组信息
        let group_row = sqlx::query(
            "SELECT group_id, group_name, member_count, diamond, footprint_capacity, settings FROM association_groups WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_optional(db)
        .await?;

        let (group_name, member_count): (String, i32) = match group_row {
            Some(r) => (r.get("group_name"), r.get("member_count")),
            None => return Err(CustomError::NotFound("群组不存在".into())),
        };

        // 获取群组成员
        let members = sqlx::query_as::<_, UserRecord>(
            r#"SELECT u.user_id, u.username, u.nick_name, u.avatar, u.role, u.love_point, u.diamond,
               (SELECT agm.group_id FROM association_group_members agm WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
               FROM users u
               JOIN association_group_members agm ON agm.user_id = u.user_id
               WHERE agm.group_id = $1"#
        )
        .bind(group_id)
        .fetch_all(db)
        .await?;

        Ok(GroupInfoOut {
            group_id,
            group_name,
            member_count,
            members: members.into_iter().map(Into::into).collect(),
        })
    }

    /// 直接绑定用户
    pub async fn bind_user_directly(
        user_id: i64,
        input: BindUserDirectlyInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        todo!("迁移直接绑定用户")
    }

    /// 更新群组
    pub async fn update_group(
        user_id: i64,
        group_id: i64,
        input: GroupUpdateInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        todo!("迁移更新群组")
    }

    /// 获取群组积分配置
    pub async fn get_group_point_config(
        user_id: i64,
        group_id: i64,
        state: &Arc<AppState>,
    ) -> Result<GroupPointConfig, CustomError> {
        let db = &state.db_pool;
        let cfg = sqlx::query_as::<_, GroupPointConfig>(
            "SELECT group_id, sign_reward_daily, sign_reward_consecutive, order_point_percent FROM group_point_configs WHERE group_id=$1"
        )
        .bind(group_id)
        .fetch_optional(db)
        .await?;

        cfg.ok_or_else(|| CustomError::NotFound("群组积分配置不存在".into()))
    }

    /// 更新群组积分配置
    pub async fn update_group_point_config(
        user_id: i64,
        group_id: i64,
        input: GroupPointConfigUpdateInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        todo!("迁移更新群组积分配置")
    }
}
