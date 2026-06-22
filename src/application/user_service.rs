// 应用服务层 - 用户服务
// 包含用户登录、注册、信息管理、组服务等业务用例

use crate::config::AppState;
use crate::domain::user::entities::UserRecord;
use crate::domain::user::{
    BindUserDirectlyInput, ConfirmInvitationInput, Gender, GroupInfoOut, GroupPointConfig,
    GroupPointConfigUpdateInput, GroupUpdateInput, InvitationListOut, InvitationRequestOut,
    IsRegisterResponse, LoginInput, LoginMethod, LoginResponse, NewInvitationInput,
    ProfileUpdateInput, RegisterInput, RoleSwitchInput, RoleSwitchResult, UnbindRequestInput,
    UserInfoResponse, UserPublic, UserRole,
};
use crate::errors::CustomError;
use crate::middlewares::jwt;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::Utc;
use password_hash::SaltString;
use rand::thread_rng;
use sqlx::Row;
use std::sync::Arc;

/// 用户应用服务
pub struct UserService;

impl UserService {
    /// 注册
    pub async fn register(input: RegisterInput, state: &Arc<AppState>) -> Result<(), CustomError> {
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

        let (pwd_hash, algo) =
            Self::hash_password(&input.password).map_err(|e| CustomError::internal(e))?;

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
            account = Self::weixin_login(code, state).await?;
        };

        let record = sqlx::query_as::<_, UserRecord>(
            r#"SELECT u.user_id, u.username, u.nick_name, u.email, u.role, u.love_point, u.diamond, u.avatar, u.phone, u.open_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.username_change, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status='ACTIVE' WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
               FROM users u WHERE u.username = $1 OR u.open_id = $1"#
        )
        .bind(&account)
        .fetch_optional(db_pool)
        .await?;

        let record = match record {
            Some(r) => r,
            None => {
                if input.weixin_code.is_some() {
                    return Err(CustomError::UserNotFound(account));
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

        // 签发 access + refresh token —— 统一走 jwt 模块,与 wechat_login 形状一致
        let (access_token, _) = jwt::issue_access(record.user_id, &state.jwt_secret)?;
        let (refresh_token, _) = jwt::issue_refresh(record.user_id, &state.jwt_secret)?;

        let _ = state.redis_cache.set_user_public(&public, 3600).await;

        Ok(LoginResponse {
            access_token,
            refresh_token,
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
                   (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status='ACTIVE' WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
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
                   (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status='ACTIVE' WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
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
    ///
    /// 使用 `COALESCE($N, col)` 模式:每个可选字段都绑定一个 `Option<T>`,
    /// 当 bind 为 `None` 时 `COALESCE` 保留列的现有值。这避免了 `format!`
    /// 拼接列名带来的占位符编号冲突和 SQL 注入面。 模式:每个可选字段都绑定一个 `Option<T>`,
    /// 当 bind 为 `None` 时 `COALESCE` 保留列的现有值。这避免了 `format!`
    /// 拼接列名带来的占位符编号冲突和 SQL 注入面。
    pub async fn change_info(
        user_id: i64,
        input: ProfileUpdateInput,
        state: &Arc<AppState>,
    ) -> Result<UserPublic, CustomError> {
        let db = &state.db_pool;

        // 如果要修改用户名,检查是否已被使用
        if let Some(ref new_username) = input.new_username {
            let exists: i64 = sqlx::query(
                "SELECT COUNT(*) FROM users WHERE username = $1 AND user_id != $2",
            )
            .bind(new_username)
            .bind(user_id)
            .fetch_one(db)
            .await?
            .get(0);
            if exists > 0 {
                return Err(CustomError::BadRequest("用户名已被使用".into()));
            }
        }

        // 验证旧密码,并准备新密码哈希(若需更新)
        let (new_hash, new_algo): (Option<String>, Option<String>) =
            if let (Some(ref old_pwd), Some(ref new_pwd)) =
                (&input.old_password, &input.new_password)
            {
                let row: Option<(Option<String>,)> =
                    sqlx::query_as("SELECT password_hash FROM users WHERE user_id = $1")
                        .bind(user_id)
                        .fetch_optional(db)
                        .await?;
                let stored_hash = row.and_then(|(h,)| h).unwrap_or_default();
                if !Self::verify_password(old_pwd, &stored_hash).unwrap_or(false) {
                    return Err(CustomError::BadRequest("旧密码错误".into()));
                }
                let (hash, algo) =
                    Self::hash_password(new_pwd).map_err(|e| CustomError::internal(e))?;
                (Some(hash), Some(algo))
            } else {
                (None, None)
            };

        // 至少要有一个待更新字段,否则视为客户端错误
        if input.nick_name.is_none()
            && input.avatar.is_none()
            && input.phone.is_none()
            && input.gender.is_none()
            && input.birthday.is_none()
            && input.new_username.is_none()
            && new_hash.is_none()
        {
            return Err(CustomError::BadRequest("没有需要更新的字段".into()));
        }

        // 静态 SQL:每个 $N 占位符与 Option 字段一一对应,
        // 不会与 WHERE user_id = $1 冲突。RETURNING 子句与
        // get_current_info 的投影保持一致,UserRecord 可直接解码。
        let rec = sqlx::query_as::<_, UserRecord>(
            r#"
            UPDATE users SET
                nick_name     = COALESCE($2, nick_name),
                avatar        = COALESCE($3, avatar),
                phone         = COALESCE($4, phone),
                gender        = COALESCE($5, gender),
                birthday      = COALESCE($6, birthday),
                username      = COALESCE($7, username),
                password_hash = COALESCE($8, password_hash),
                password_algo = COALESCE($9, password_algo)
            WHERE user_id = $1
            RETURNING user_id, username, email, nick_name, role, love_point, diamond,
                      avatar, phone, open_id, status, created_at, updated_at,
                      password_hash, password_algo, gender, birthday, username_change,
                      login_method, last_login_at, password_updated_at, is_temp_password,
                      push_id, last_role_switch_at,
                      (SELECT agm.group_id FROM association_group_members agm
                         JOIN association_groups g ON g.group_id = agm.group_id AND g.status = 'ACTIVE'
                         WHERE agm.user_id = users.user_id
                         ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
            "#,
        )
        .bind(user_id)
        .bind(&input.nick_name)
        .bind(&input.avatar)
        .bind(&input.phone)
        .bind(input.gender)
        .bind(&input.birthday)
        .bind(&input.new_username)
        .bind(new_hash)
        .bind(new_algo)
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
               AND status IN ('CREATED', 'ACCEPTED', 'PRODUCTION_COMPLETED')"#,
        )
        .bind(user_id)
        .fetch_one(db)
        .await?
        .get(0);

        if active_orders > 0 {
            return Err(CustomError::BadRequest(
                "有正在进行的订单，无法切换角色".into(),
            ));
        }

        // 检查24小时内是否切换过
        let last_switch: Option<chrono::DateTime<Utc>> =
            sqlx::query("SELECT last_role_switch_at FROM users WHERE user_id = $1")
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
        sqlx::query("UPDATE users SET role = $2, last_role_switch_at = $3 WHERE user_id = $1")
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

    async fn weixin_login(code: &str, state: &Arc<AppState>) -> Result<String, CustomError> {
        let app_id = state.wx_app_id.clone();
        let app_secret = state.wx_app_secret.clone();

        if app_id.is_empty() || app_secret.is_empty() {
            return Err(CustomError::internal(String::from(
                "微信登录未配置(WX_APP_ID / WX_APP_SECRET 缺失)",
            )));
        }

        let res = reqwest::get(
            "https://api.weixin.qq.com/sns/jscode2session?grant_type=authorization_code&appid="
                .to_string()
                + &app_id
                + "&secret="
                + &app_secret
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
    /// 获取邀请列表 - Note: invitations table doesn't exist in v3.sql
    /// This functionality is not implemented
    #[allow(dead_code)]
    pub async fn get_invitation(
        _user_id: i64,
        _state: &Arc<AppState>,
    ) -> Result<InvitationListOut, CustomError> {
        Ok(InvitationListOut {
            incoming: vec![],
            outgoing: vec![],
        })
    }

    /// 创建邀请 - Note: invitations table doesn't exist in v3.sql
    /// This functionality is not implemented
    #[allow(dead_code)]
    pub async fn new_invitation(
        _user_id: i64,
        _input: NewInvitationInput,
        _state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        Err(CustomError::NotFound("邀请功能暂未实现".into()))
    }

    /// 确认邀请 - Note: invitations table doesn't exist in v3.sql
    #[allow(dead_code)]
    pub async fn confirm_invitation(
        _user_id: i64,
        _invitation_id: i64,
        _input: ConfirmInvitationInput,
        _state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        Err(CustomError::NotFound("邀请功能暂未实现".into()))
    }

    /// 取消邀请 - Note: invitations table doesn't exist in v3.sql
    #[allow(dead_code)]
    pub async fn cancel_invitation(
        _invitation_id: i64,
        _state: &Arc<AppState>,
    ) -> Result<(), CustomError> {
        Err(CustomError::NotFound("邀请功能暂未实现".into()))
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
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = 1"
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        let group_id = match membership {
            Some((gid,)) => gid,
            None => return Err(CustomError::BadRequest("您不在任何组中".into())),
        };

        // 检查组内其他成员
        let member_count: i64 =
            sqlx::query("SELECT COUNT(*) FROM association_group_members WHERE group_id = $1")
                .bind(group_id)
                .fetch_one(db)
                .await?
                .get(0);

        if member_count <= 1 {
            return Err(CustomError::BadRequest("组内只剩您一人，无需解绑".into()));
        }

        // Note: unbind_requests table doesn't exist in v3.sql
        // Only remove user from group (association_group_members DELETE exists)
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

        // 获取群组信息 (member_count derived dynamically)
        let group_row = sqlx::query(
            "SELECT group_id, group_name, diamond, footprint_capacity, settings FROM association_groups WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_optional(db)
        .await?;

        let (group_name,): (String,) = match group_row {
            Some(r) => (r.get("group_name"),),
            None => return Err(CustomError::NotFound("群组不存在".into())),
        };

        // 获取组成员数量
        let member_count: i32 =
            sqlx::query("SELECT COUNT(*) FROM association_group_members WHERE group_id = $1 AND member_status='ACTIVE'")
                .bind(group_id)
                .fetch_one(db)
                .await?
                .get::<i64, _>(0) as i32;

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
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = 1"
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        let group_id = match user_group_id {
            Some((gid,)) => gid,
            None => return Err(CustomError::BadRequest("您不在任何组中".into())),
        };

        // 将目标用户绑定到组
        // is_primary 是 smallint,不能写 true
        sqlx::query(
            r#"INSERT INTO association_group_members (user_id, group_id, is_primary, joined_at)
               VALUES ($1, $2, 1, $3)
               ON CONFLICT (user_id) DO UPDATE SET group_id = $2, is_primary = 1"#
        )
        .bind(input.user_id)
        .bind(group_id)
        .bind(Utc::now())
        .execute(db)
        .await?;

        // Note: member_count is not a real column - group membership is counted dynamically
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
            "SELECT COUNT(*) FROM association_group_members WHERE user_id = $1 AND group_id = $2",
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
    #[allow(dead_code)]
    pub async fn get_group_point_config(
        _user_id: i64,
        group_id: i64,
        state: &Arc<AppState>,
    ) -> Result<GroupPointConfig, CustomError> {
        let db = &state.db_pool;
        let cfg = sqlx::query_as::<_, GroupPointConfig>(
            "SELECT group_id, order_point_percent FROM group_point_configs WHERE group_id=$1"
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
        let user_role: Option<(String,)> =
            sqlx::query_as("SELECT role::text FROM users WHERE user_id = $1")
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
            .bind(input.order_point_percent)
            .bind(group_id)
            .execute(db)
            .await?;

        Ok(())
    }
}
