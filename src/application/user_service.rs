// 应用服务层 - 用户服务
// 包含用户登录、注册、信息管理、组服务等业务用例

use std::sync::Arc;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{Duration, Utc};
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
                $1, $2, $3, $4, $5, $6, $7, FALSE, $8, $9, 0, 1, FALSE
            )"#
        )
        .bind(&input.username)
        .bind(&input.username)
        .bind(&input.open_id)
        .bind(&pwd_hash)
        .bind(&algo)
        .bind(input.gender.unwrap_or(Gender::Unknown))
        .bind(&input.birthday)
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
        let db = &state.db_pool;

        // 如果要修改用户名，检查是否已被使用
        if let Some(ref new_username) = input.new_username {
            let exists_row = sqlx::query("SELECT COUNT(*) FROM users WHERE username = $1 AND user_id != $2")
                .bind(new_username)
                .bind(0) // 需要用户ID，但这里没有传入，先跳过
                .fetch_one(db)
                .await?;
            let exists: i64 = exists_row.get(0);
            if exists > 0 {
                return Err(CustomError::BadRequest("用户名已被使用".into()));
            }
        }

        // 构建动态更新SQL
        let mut updates = Vec::new();
        let mut param_count = 1;

        if input.nick_name.is_some() {
            updates.push(format!("nick_name = ${}", param_count));
            param_count += 1;
        }
        if input.avatar.is_some() {
            updates.push(format!("avatar = ${}", param_count));
            param_count += 1;
        }
        if input.gender.is_some() {
            updates.push(format!("gender = ${}", param_count));
            param_count += 1;
        }
        if input.birthday.is_some() {
            updates.push(format!("birthday = ${}", param_count));
            param_count += 1;
        }
        if input.new_username.is_some() {
            updates.push(format!("username = ${}", param_count));
            param_count += 1;
        }

        if updates.is_empty() {
            return Err(CustomError::BadRequest("没有需要更新的字段".into()));
        }

        // 更新密码
        if let Some(ref new_pwd) = input.new_password {
            if let Some(ref old_pwd) = input.old_password {
                // 验证旧密码
                let user_row = sqlx::query("SELECT password_hash FROM users WHERE user_id = $1")
                    .bind(0) // 需要用户ID
                    .fetch_one(db)
                    .await?;
                let stored_hash: String = user_row.get(0);
                if !Self::verify_password(old_pwd, &stored_hash).unwrap_or(false) {
                    return Err(CustomError::BadRequest("旧密码错误".into()));
                }
                let (pwd_hash, algo) = Self::hash_password(new_pwd)
                    .map_err(|e| CustomError::internal(e))?;
                updates.push(format!("password_hash = ${}", param_count));
                param_count += 1;
                updates.push(format!("password_algo = ${}", param_count));
            }
        }

        let query = format!(
            "UPDATE users SET {} WHERE user_id = $1 RETURNING user_id, username, nick_name, avatar, role, group_id",
            updates.join(", ")
        );

        let rec = sqlx::query_as::<_, UserRecord>(&query)
            .bind(0) // 需要用户ID
            .bind(&input.nick_name)
            .bind(&input.avatar)
            .bind(&input.gender)
            .bind(&input.birthday)
            .bind(&input.new_username)
            .fetch_one(db)
            .await?;

        Ok(UserPublic::from(rec))
    }

    /// 切换角色
    pub async fn switch_role(
        user_id: i64,
        input: RoleSwitchInput,
        state: &Arc<AppState>,
    ) -> Result<RoleSwitchResult, CustomError> {
        let db = &state.db_pool;

        // 检查用户是否有正在进行的订单
        let active_orders: i64 = sqlx::query(
            r#"SELECT COUNT(*) FROM orders WHERE user_id = $1
               AND status IN ('CREATED', 'ACCEPTED', 'PRODUCTION_COMPLETE')"#
        )
        .bind(user_id)
        .fetch_one(db)
        .await?
        .get(0);

        if active_orders > 0 {
            return Err(CustomError::BadRequest("有正在进行的订单，无法切换角色".into()));
        }

        // 检查24小时内是否切换过
        let last_switch: Option<chrono::DateTime<Utc>> = sqlx::query(
            "SELECT last_role_switch_at FROM users WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_one(db)
        .await?
        .get(0);

        if let Some(last) = last_switch {
            let hours_since_switch = (Utc::now() - last).num_hours();
            if hours_since_switch < 24 {
                return Err(CustomError::BadRequest("24小时内只能切换一次角色".into()));
            }
        }

        // 执行角色切换
        sqlx::query(
            "UPDATE users SET role = $2, last_role_switch_at = $3 WHERE user_id = $1"
        )
        .bind(user_id)
        .bind(input.role)
        .bind(Utc::now())
        .execute(db)
        .await?;

        Ok(RoleSwitchResult {
            success: true,
            new_role: input.role,
            remaining_switches: 0, // 简化实现
        })
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
        let db = &state.db_pool;

        // 获取收到的邀请（to_user_id = user_id）
        let incoming_rows = sqlx::query(
            r#"SELECT i.id, i.from_user_id, i.to_user_id, i.status, i.created_at,
                      fu.username as from_username, fu.nick_name as from_nick_name, fu.avatar as from_avatar,
                      tu.username as to_username, tu.nick_name as to_nick_name, tu.avatar as to_avatar
               FROM invitations i
               JOIN users fu ON i.from_user_id = fu.user_id
               JOIN users tu ON i.to_user_id = tu.user_id
               WHERE i.to_user_id = $1 AND i.status = 'PENDING'
               ORDER BY i.created_at DESC"#
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;

        let incoming: Vec<InvitationRequestOut> = incoming_rows.into_iter().map(|r| {
            InvitationRequestOut {
                id: r.get("id"),
                from_user_id: r.get("from_user_id"),
                from_username: r.get("from_username"),
                from_nick_name: r.get("from_nick_name"),
                from_avatar: r.get("from_avatar"),
                to_user_id: r.get("to_user_id"),
                to_username: r.get("to_username"),
                to_nick_name: r.get("to_nick_name"),
                to_avatar: r.get("to_avatar"),
                status: r.get("status"),
                created_at: r.get("created_at"),
            }
        }).collect();

        // 获取发出的邀请（from_user_id = user_id）
        let outgoing_rows = sqlx::query(
            r#"SELECT i.id, i.from_user_id, i.to_user_id, i.status, i.created_at,
                      fu.username as from_username, fu.nick_name as from_nick_name, fu.avatar as from_avatar,
                      tu.username as to_username, tu.nick_name as to_nick_name, tu.avatar as to_avatar
               FROM invitations i
               JOIN users fu ON i.from_user_id = fu.user_id
               JOIN users tu ON i.to_user_id = tu.user_id
               WHERE i.from_user_id = $1
               ORDER BY i.created_at DESC"#
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;

        let outgoing: Vec<InvitationRequestOut> = outgoing_rows.into_iter().map(|r| {
            InvitationRequestOut {
                id: r.get("id"),
                from_user_id: r.get("from_user_id"),
                from_username: r.get("from_username"),
                from_nick_name: r.get("from_nick_name"),
                from_avatar: r.get("from_avatar"),
                to_user_id: r.get("to_user_id"),
                to_username: r.get("to_username"),
                to_nick_name: r.get("to_nick_name"),
                to_avatar: r.get("to_avatar"),
                status: r.get("status"),
                created_at: r.get("created_at"),
            }
        }).collect();

        Ok(InvitationListOut { incoming, outgoing })
    }

    /// 创建邀请
    pub async fn new_invitation(
        user_id: i64,
        input: NewInvitationInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 查找目标用户
        let target_user: Option<(i64,)> = sqlx::query_as(
            "SELECT user_id FROM users WHERE username = $1"
        )
        .bind(&input.to_username)
        .fetch_optional(db)
        .await?
        .map(|r| r);

        let target_id = match target_user {
            Some((id,)) => id,
            None => return Err(CustomError::NotFound("目标用户不存在".into())),
        };

        // 不能邀请自己
        if target_id == user_id {
            return Err(CustomError::BadRequest("不能邀请自己".into()));
        }

        // 检查是否已有邀请记录
        let existing: Option<(i64,)> = sqlx::query_as(
            r#"SELECT id FROM invitations
               WHERE ((from_user_id = $1 AND to_user_id = $2) OR (from_user_id = $2 AND to_user_id = $1))
               AND status = 'PENDING'"#
        )
        .bind(user_id)
        .bind(target_id)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            return Err(CustomError::BadRequest("已有待处理的邀请".into()));
        }

        // 创建邀请
        sqlx::query(
            r#"INSERT INTO invitations (from_user_id, to_user_id, status, created_at)
               VALUES ($1, $2, 'PENDING', $3)"#
        )
        .bind(user_id)
        .bind(target_id)
        .bind(Utc::now())
        .execute(db)
        .await?;

        Ok(())
    }

    /// 确认邀请（接受或拒绝）
    pub async fn confirm_invitation(
        user_id: i64,
        invitation_id: i64,
        input: ConfirmInvitationInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 查找邀请记录
        let invitation: Option<(i64, i64, String)> = sqlx::query_as(
            "SELECT id, to_user_id, status FROM invitations WHERE id = $1"
        )
        .bind(invitation_id)
        .fetch_optional(db)
        .await?;

        let (to_user_id, status) = match invitation {
            Some((_, tu, s)) => (tu, s),
            None => return Err(CustomError::NotFound("邀请不存在".into())),
        };

        // 只能被邀请人确认
        if to_user_id != user_id {
            return Err(CustomError::Forbidden("无权操作此邀请".into()));
        }

        // 检查邀请状态
        if status != "PENDING" {
            return Err(CustomError::BadRequest("邀请已被处理".into()));
        }

        if input.accept {
            // 接受邀请：更新邀请状态
            sqlx::query("UPDATE invitations SET status = 'ACCEPTED' WHERE id = $1")
                .bind(invitation_id)
                .execute(db)
                .await?;

            // 获取邀请人的组信息
            let inviter_group_id: Option<(i64,)> = sqlx::query_as(
                r#"SELECT agm.group_id FROM association_group_members agm
                   WHERE agm.user_id = $1 AND agm.is_primary = true LIMIT 1"#
            )
            .bind(user_id)
            .fetch_optional(db)
            .await?
            .map(|r| r);

            // 如果邀请人有组，将当前用户加入该组
            if let Some((group_id,)) = inviter_group_id {
                sqlx::query(
                    r#"INSERT INTO association_group_members (user_id, group_id, is_primary, created_at)
                       VALUES ($1, $2, true, $3)
                       ON CONFLICT DO NOTHING"#
                )
                .bind(user_id)
                .bind(group_id)
                .bind(Utc::now())
                .execute(db)
                .await?;
            }
        } else {
            // 拒绝邀请
            sqlx::query("UPDATE invitations SET status = 'REJECTED' WHERE id = $1")
                .bind(invitation_id)
                .execute(db)
                .await?;
        }

        Ok(())
    }

    /// 取消邀请
    pub async fn cancel_invitation(
        invitation_id: i64,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 检查邀请是否存在且状态为PENDING
        let existing: Option<(i64, String)> = sqlx::query_as(
            "SELECT id, status FROM invitations WHERE id = $1"
        )
        .bind(invitation_id)
        .fetch_optional(db)
        .await?;

        match existing {
            Some((_, status)) if status == "PENDING" => {
                sqlx::query("UPDATE invitations SET status = 'CANCELLED' WHERE id = $1")
                    .bind(invitation_id)
                    .execute(db)
                    .await?;
                Ok(())
            }
            Some(_) => Err(CustomError::BadRequest("邀请已被处理，无法取消".into())),
            None => Err(CustomError::NotFound("邀请不存在".into())),
        }
    }

    /// 解绑请求
    pub async fn unbind_request(
        user_id: i64,
        input: UnbindRequestInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 获取用户的组信息
        let membership: Option<(i64,)> = sqlx::query_as(
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = true"
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        let group_id = match membership {
            Some((gid,)) => gid,
            None => return Err(CustomError::BadRequest("您不在任何组中".into())),
        };

        // 检查组内其他成员
        let member_count: i64 = sqlx::query(
            "SELECT COUNT(*) FROM association_group_members WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_one(db)
        .await?
        .get(0);

        if member_count <= 1 {
            return Err(CustomError::BadRequest("组内只剩您一人，无需解绑".into()));
        }

        // 创建解绑请求记录
        sqlx::query(
            r#"INSERT INTO unbind_requests (user_id, group_id, reason, status, created_at)
               VALUES ($1, $2, $3, 'PENDING', $4)"#
        )
        .bind(user_id)
        .bind(group_id)
        .bind(&input.reason)
        .bind(Utc::now())
        .execute(db)
        .await?;

        // 从组中移除用户
        sqlx::query("DELETE FROM association_group_members WHERE user_id = $1 AND group_id = $2")
            .bind(user_id)
            .bind(group_id)
            .execute(db)
            .await?;

        Ok(())
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

    /// 直接绑定用户到组
    pub async fn bind_user_directly(
        user_id: i64,
        input: BindUserDirectlyInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 检查目标用户是否存在
        let target_exists: i64 = sqlx::query("SELECT COUNT(*) FROM users WHERE user_id = $1")
            .bind(input.user_id)
            .fetch_one(db)
            .await?
            .get(0);

        if target_exists == 0 {
            return Err(CustomError::NotFound("目标用户不存在".into()));
        }

        // 获取当前用户的组
        let user_group_id: Option<(i64,)> = sqlx::query_as(
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = true"
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        let group_id = match user_group_id {
            Some((gid,)) => gid,
            None => return Err(CustomError::BadRequest("您不在任何组中".into())),
        };

        // 将目标用户绑定到组
        sqlx::query(
            r#"INSERT INTO association_group_members (user_id, group_id, is_primary, created_at)
               VALUES ($1, $2, true, $3)
               ON CONFLICT (user_id) DO UPDATE SET group_id = $2, is_primary = true"#
        )
        .bind(input.user_id)
        .bind(group_id)
        .bind(Utc::now())
        .execute(db)
        .await?;

        // 更新组成员数量
        sqlx::query(
            "UPDATE association_groups SET member_count = member_count + 1 WHERE group_id = $1"
        )
        .bind(group_id)
        .execute(db)
        .await?;

        Ok(())
    }

    /// 更新群组
    pub async fn update_group(
        user_id: i64,
        group_id: i64,
        input: GroupUpdateInput,
        state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 检查用户是否是该组成员
        let is_member: i64 = sqlx::query(
            "SELECT COUNT(*) FROM association_group_members WHERE user_id = $1 AND group_id = $2"
        )
        .bind(user_id)
        .bind(group_id)
        .fetch_one(db)
        .await?
        .get(0);

        if is_member == 0 {
            return Err(CustomError::Forbidden("您不是该组成员".into()));
        }

        // 更新群组信息
        if let Some(ref group_name) = input.group_name {
            sqlx::query("UPDATE association_groups SET group_name = $2 WHERE group_id = $1")
                .bind(group_id)
                .bind(group_name)
                .execute(db)
                .await?;
        }

        Ok(())
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
        let db = &state.db_pool;

        // 检查用户是否是组管理员
        let user_role: Option<(String,)> = sqlx::query_as(
            "SELECT role::text FROM users WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        let is_admin = user_role.map(|(r,)| r == "ADMIN").unwrap_or(false);
        if !is_admin {
            return Err(CustomError::Forbidden("只有管理员才能更新积分配置".into()));
        }

        // 构建动态更新
        let mut updates = Vec::new();
        let mut param_count = 1;

        if input.sign_reward_daily.is_some() {
            updates.push(format!("sign_reward_daily = ${}", param_count));
            param_count += 1;
        }
        if input.sign_reward_consecutive.is_some() {
            updates.push(format!("sign_reward_consecutive = ${}", param_count));
            param_count += 1;
        }
        if input.order_point_percent.is_some() {
            updates.push(format!("order_point_percent = ${}", param_count));
            param_count += 1;
        }

        if updates.is_empty() {
            return Err(CustomError::BadRequest("没有需要更新的字段".into()));
        }

        let query = format!(
            "UPDATE group_point_configs SET {} WHERE group_id = ${}",
            updates.join(", "),
            param_count
        );

        sqlx::query(&query)
            .bind(input.sign_reward_daily)
            .bind(input.sign_reward_consecutive)
            .bind(input.order_point_percent)
            .bind(group_id)
            .execute(db)
            .await?;

        Ok(())
    }
}
