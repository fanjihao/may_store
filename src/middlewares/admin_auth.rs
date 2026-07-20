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

use crate::{
    config::AppState,
    domain::user::UserPublic,
    errors::CustomError,
    middlewares::{auth::load_user_public_for_token, jwt},
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
        let state = req.app_state::<Arc<AppState>>().cloned();
        let auth_header = req.headers().get("Authorization").cloned();

        async move {
            let state =
                state.ok_or_else(|| CustomError::internal(String::from("app state 缺失")))?;
            let mut raw = auth_header
                .ok_or_else(|| CustomError::auth_invalid_token("缺少 Authorization 头"))?
                .to_str()
                .map_err(|_| CustomError::auth_invalid_token("Authorization 头格式非法"))?
                .to_string();
            if let Some(stripped) = raw.strip_prefix("Bearer ") {
                raw = stripped.trim().to_string();
            }

            // 1. 统一 JWT 校验:签名 / 算法 / exp / iss / typ / 黑名单 / 全撤销
            let claims = jwt::verify(
                &raw,
                &state.jwt_secret,
                jwt::TokenType::Access,
                &state.redis_cache,
            )
            .await?;
            let user_id = claims.user_id()?;

            // 2. users 主账号也必须为 ACTIVE；DB 状态检查失败时拒绝管理员请求
            let user = Some(load_user_public_for_token(&state, user_id).await?);

            // 3. 查 admin_users 表 —— UserRole::Admin 不被信任,必须以表为准
            let row: Option<(i64, String, String)> = sqlx::query_as(
                r#"SELECT admin_id, role::text, status
                   FROM admin_users
                   WHERE user_id = $1"#,
            )
            .bind(user_id)
            .fetch_optional(&state.db_pool)
            .await?;

            let (admin_id, role_str, status) =
                row.ok_or_else(|| CustomError::Forbidden(String::from("需要管理员权限")))?;

            if status != "ACTIVE" {
                return Err(CustomError::Forbidden(String::from("管理员账号已停用")));
            }

            let role = AdminRole::from_str(&role_str)
                .ok_or_else(|| CustomError::internal(format!("未知管理员角色: {role_str}")))?;

            Ok(AdminToken {
                admin_id,
                user_id,
                role,
                user,
            })
        }
    }
}
