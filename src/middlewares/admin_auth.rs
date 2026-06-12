//! AdminToken —— 后台管理员鉴权提取器
//!
//! FSD §11.24:`admin_users` 表 + `admin_role_enum`
//!
//! 关键点:与 UserToken 不同,**不能**信任 `UserRole::Admin`(组内业务角色)。
//! 必须是 `admin_users` 表中存在且 `status='ACTIVE'` 的记录。
//!
//! 用法:handler 加 `admin: AdminToken` 参数。

use std::{future::Future, sync::Arc};

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use redis::AsyncCommands;

use crate::{
    config::AppState,
    domain::user::UserPublic,
    errors::CustomError,
    middlewares::auth::{UserTokenClaims, decode_jwt, load_user_public_for_token},
};

/// 管理员角色 (FSD §11.24)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminRole {
    SuperAdmin,
    Ops,
    RiskReviewer,
}

impl AdminRole {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "SUPER_ADMIN" => Some(Self::SuperAdmin),
            "OPS" => Some(Self::Ops),
            "RISK_REVIEWER" => Some(Self::RiskReviewer),
            _ => None,
        }
    }

    pub fn is_super(&self) -> bool {
        matches!(self, Self::SuperAdmin)
    }
}

#[derive(Debug, Clone)]
pub struct AdminToken {
    pub admin_id: i64,
    pub user_id: i64,
    pub role: AdminRole,
    pub user: Option<UserPublic>,
}

impl<E: ErrorRenderer> FromRequest<E> for AdminToken {
    type Error = CustomError;

    fn from_request(
        req: &HttpRequest,
        _payload: &mut Payload,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        // 取出 state、auth header、redis
        let state = req.app_state::<Arc<AppState>>().cloned();
        let auth_header = req.headers().get("Authorization").cloned();

        async move {
            let state = state.ok_or_else(|| {
                CustomError::internal(String::from("app state 缺失"))
            })?;
            let mut raw = auth_header
                .ok_or_else(|| CustomError::unauthorized("No login authorization"))?
                .to_str()
                .map_err(|_| CustomError::unauthorized("Invalid header"))?
                .to_string();
            if let Some(stripped) = raw.strip_prefix("Bearer ") {
                raw = stripped.trim().to_string();
            }

            // 1. JWT 解码
            let claims: UserTokenClaims = decode_jwt(&raw, &state.jwt_secret)?;

            // 2. 检查 token 黑名单
            let blacklist_key = format!("token_blacklist:{}:{}", claims.user_id, claims.exp);
            let mut conn = state
                .redis_cache
                .get_conn()
                .await
                .map_err(|e| CustomError::internal(format!("Redis 连接失败: {e}")))?;
            let is_revoked: Option<String> = conn.get(&blacklist_key).await.unwrap_or(None);
            if is_revoked.is_some() {
                return Err(CustomError::unauthorized("token 已被撤销"));
            }

            // 3. 加载用户公开信息(可选,失败不阻塞)
            let user = load_user_public_for_token(&state, claims.user_id).await;

            // 4. 查 admin_users 表
            let row: Option<(i64, String, String)> = sqlx::query_as(
                r#"SELECT admin_id, role::text, status
                   FROM admin_users
                   WHERE user_id = $1"#,
            )
            .bind(claims.user_id)
            .fetch_optional(&state.db_pool)
            .await?;

            let (admin_id, role_str, status) = row
                .ok_or_else(|| CustomError::Forbidden(String::from("需要管理员权限")))?;

            if status != "ACTIVE" {
                return Err(CustomError::Forbidden(String::from("管理员账号已停用")));
            }

            let role = AdminRole::from_str(&role_str).ok_or_else(|| {
                CustomError::internal(format!("未知管理员角色: {role_str}"))
            })?;

            Ok(AdminToken {
                admin_id,
                user_id: claims.user_id,
                role,
                user,
            })
        }
    }
}
