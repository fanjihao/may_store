use std::{future::Future, sync::Arc};

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use serde::{Deserialize, Serialize};

use crate::config::AppState;
use crate::domain::user::{Gender, LoginMethod, UserPublic, UserRecord, UserRole};
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
    /// 用户公开信息(从 Redis 缓存或 DB 加载,可能为 None)
    pub user: Option<UserPublic>,
}

/// 公开的用户公开信息加载工具 —— AdminToken 等其他提取器复用
pub async fn load_user_public_for_token(
    state: &AppState,
    user_id: i64,
) -> Option<UserPublic> {
    if let Ok(Some(p)) = state.redis_cache.get_user_public(&user_id).await {
        return Some(p);
    }
    let db = &state.db_pool;
    if let Ok(record) = sqlx::query_as!(
        UserRecord,
        r#"
        SELECT u.user_id, u.username, u.email, u.nick_name,
               u.role::text AS "role!: UserRole",
               u.love_point, u.diamond,
               u.avatar AS "avatar!: Option<String>",
               u.phone, u.open_id, u.created_at, u.updated_at, u.password_hash, u.password_algo,
               u.gender::text AS "gender!: Gender",
               u.birthday,
               u.username_change AS "username_change!: Option<bool>",
               u.login_method::text AS "login_method!: LoginMethod",
               u.last_login_at, u.password_updated_at,
               u.is_temp_password AS "is_temp_password!: Option<bool>",
               u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm
                  JOIN association_groups g ON g.group_id=agm.group_id AND g.status='ACTIVE'::user_status_enum
                  WHERE agm.user_id=u.user_id AND agm.member_status='ACTIVE'::group_member_status_enum
                  ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS "group_id!: Option<i64>"
        FROM users u WHERE u.user_id=$1 AND u.status='ACTIVE'
        "#,
        user_id
    )
    .fetch_one(db)
    .await
    {
        let p: UserPublic = record.into();
        let _ = state.redis_cache.set_user_public(&p, 3600).await;
        return Some(p);
    }
    None
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

            // 加载用户公开信息(可选,失败不阻塞)
            let public = load_user_public_for_token(&state, user_id).await;

            if let Some(ref p) = public {
                req_for_ext.extensions_mut().insert::<UserPublic>(p.clone());
            }

            Ok(UserToken {
                exp: claims.exp,
                jti: claims.jti,
                user_id,
                user: public,
            })
        }
    }
}
