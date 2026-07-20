use std::{future::Future, sync::Arc};

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use serde::{Deserialize, Serialize};

use crate::config::AppState;
use crate::domain::user::{UserPublic, UserRecord};
use crate::errors::CustomError;
use crate::middlewares::jwt;

/// HTTP 处理器从 `Authorization: Bearer <jwt>` 中解析出的已认证用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserToken {
    /// Token 过期时间 (Unix 秒) —— logout 时用于计算黑名单 TTL
    pub exp: i64,
    /// Token 唯一 ID —— logout 时用于精确撤销
    pub jti: String,
    /// 用户 ID
    pub user_id: i64,
    /// ACTIVE 用户的公开信息（账号状态始终以 DB 为准）
    pub user: UserPublic,
}

/// 对外统一使用的账号不可用提示，不区分封禁、注销或账号缺失。
pub(crate) const ACCOUNT_UNAVAILABLE_MESSAGE: &str = "账号当前无法使用，请联系客服";

fn validate_account_status(status: Option<&str>) -> Result<(), CustomError> {
    match status {
        Some("ACTIVE") => Ok(()),
        Some("BANNED") | Some("DELETED") | None => Err(CustomError::auth_account_banned(
            ACCOUNT_UNAVAILABLE_MESSAGE,
        )),
        Some(other) => {
            log::error!("users.status 出现未知值: {}", other);
            Err(CustomError::internal("账号状态校验失败"))
        }
    }
}

/// 直接查询 DB 校验账号仍为 ACTIVE。
///
/// 该检查不能由 `user:{id}` 的一小时缓存替代；数据库错误会原样向上传递，
/// 不会被折叠成“账号不存在”。
pub async fn ensure_active_account(state: &AppState, user_id: i64) -> Result<(), CustomError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status::text FROM users WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&state.db_pool)
            .await?;

    validate_account_status(status.as_deref())
}

/// 公开的 ACTIVE 用户信息加载工具 —— AdminToken 等其他提取器复用
pub async fn load_user_public_for_token(
    state: &AppState,
    user_id: i64,
) -> Result<UserPublic, CustomError> {
    // 状态必须逐次查 DB；Redis 中的 UserPublic 不包含 status，且最长缓存一小时。
    ensure_active_account(state, user_id).await?;

    match state.redis_cache.get_user_public(&user_id).await {
        Ok(Some(p)) => return Ok(p),
        Ok(None) => {}
        Err(e) => {
            // 用户资料缓存故障可回源 DB，但绝不绕过上面的 DB 状态检查。
            log::warn!("读取用户 {} 的公开信息缓存失败: {}", user_id, e);
        }
    }

    let db = &state.db_pool;
    let record = sqlx::query_as::<_, UserRecord>(
        r#"
        SELECT u.user_id, u.username, u.email, u.nick_name,
               u.role,
               u.love_point, u.diamond,
               u.avatar,
               u.phone, u.open_id, u.created_at, u.updated_at, u.password_hash, u.password_algo,
               u.gender,
               u.birthday,
               u.username_change,
               u.login_method,
               u.last_login_at, u.password_updated_at,
               u.is_temp_password,
               u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm
                  JOIN association_groups g ON g.group_id=agm.group_id AND g.status='ACTIVE'::user_status_enum
                  WHERE agm.user_id=u.user_id AND agm.member_status='ACTIVE'::group_member_status_enum
                  ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
        FROM users u WHERE u.user_id=$1 AND u.status='ACTIVE'
        "#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::auth_account_banned(ACCOUNT_UNAVAILABLE_MESSAGE))?;

    let public: UserPublic = record.into();
    if let Err(e) = state.redis_cache.set_user_public(&public, 3600).await {
        log::warn!("写入用户 {} 的公开信息缓存失败: {}", user_id, e);
    }
    Ok(public)
}

impl<E: ErrorRenderer> FromRequest<E> for UserToken {
    type Error = CustomError;

    fn from_request(
        req: &HttpRequest,
        _: &mut Payload,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        let state = req.app_state::<Arc<AppState>>().expect("app state").clone();
        let auth_header = req.headers().get("Authorization").cloned();
        let req_for_ext = req.clone();

        async move {
            let mut raw = auth_header
                .ok_or_else(|| CustomError::auth_invalid_token("缺少 Authorization 头"))?
                .to_str()
                .map_err(|_| CustomError::auth_invalid_token("Authorization 头格式非法"))?
                .to_string();
            // 支持 'Bearer <token>' 前缀
            if let Some(stripped) = raw.strip_prefix("Bearer ") {
                raw = stripped.trim().to_string();
            }

            // 一站式校验:签名 / 算法 / exp / iss / typ / 黑名单 / 全设备撤销
            let claims = jwt::verify(
                &raw,
                &state.jwt_secret,
                jwt::TokenType::Access,
                &state.redis_cache,
            )
            .await?;
            let user_id = claims.user_id()?;

            // DB 状态校验 + 公开信息加载；非 ACTIVE、账号缺失或 DB 故障均拒绝。
            let public = load_user_public_for_token(&state, user_id).await?;

            req_for_ext
                .extensions_mut()
                .insert::<UserPublic>(public.clone());

            Ok(UserToken {
                exp: claims.exp,
                jti: claims.jti,
                user_id,
                user: public,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_status_is_accepted() {
        assert!(validate_account_status(Some("ACTIVE")).is_ok());
    }

    #[test]
    fn unavailable_statuses_share_generic_error() {
        for status in [Some("BANNED"), Some("DELETED"), None] {
            let err = validate_account_status(status).unwrap_err();
            assert!(matches!(&err, CustomError::AuthAccountBanned(_)));
            assert_eq!(err.message(), ACCOUNT_UNAVAILABLE_MESSAGE);
        }
    }

    #[test]
    fn unknown_status_is_internal_error() {
        let err = validate_account_status(Some("UNKNOWN")).unwrap_err();
        assert!(matches!(err, CustomError::InternalServerError(_)));
    }
}
