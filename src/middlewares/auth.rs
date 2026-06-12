use std::{future::Future, sync::Arc};

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use serde::{Deserialize, Serialize};

use crate::config::AppState;
use crate::domain::user::{Gender, LoginMethod, UserPublic, UserRecord, UserRole};
use crate::errors::CustomError;

// ========== Token Claims ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTokenClaims {
    pub exp: i64,
    // token 生成时用的是 `sub`（JWT 标准 claim），这里用 alias 兼容 `sub` 和 `user_id` 两种命名
    #[serde(alias = "sub")]
    pub user_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserToken {
    pub exp: i64,
    pub user_id: i64,
    pub user: Option<UserPublic>,
}

/// 公开的 JWT 解码工具 —— 给 AdminToken 等其他提取器复用
pub fn decode_jwt(token: &str, secret: &str) -> Result<UserTokenClaims, CustomError> {
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::new(Algorithm::HS256);
    decode::<UserTokenClaims>(token, &decoding_key, &validation)
        .map(|d| d.claims)
        .map_err(|e| CustomError::unauthorized(format!("decode token error: {}", e)))
}

/// 公开的用户公开信息加载工具
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
                  JOIN association_groups g ON g.group_id=agm.group_id AND g.status='ACTIVE'
                  WHERE agm.user_id=u.user_id
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
        let redis_cache = state.redis_cache.clone();
        let auth_header = req.headers().get("Authorization").cloned();

        async move {
            let mut raw = auth_header
                .ok_or_else(|| CustomError::unauthorized("No login authorization"))?
                .to_str()
                .map_err(|_| CustomError::unauthorized("Invalid header"))?
                .to_string();
            // 支持 'Bearer <token>' 前缀
            if let Some(stripped) = raw.strip_prefix("Bearer ") {
                raw = stripped.trim().to_string();
            }

            let decoding_key = DecodingKey::from_secret(state.jwt_secret.as_bytes());
            let validation = Validation::new(Algorithm::HS256);
            let data = decode::<UserTokenClaims>(&raw, &decoding_key, &validation)
                .map_err(|e| CustomError::unauthorized(format!("decode token error: {}", e)))?;
            let uid = data.claims.user_id;

            // 检查 token 黑名单(logout 后被加入)
            let blacklist_key = format!("token_blacklist:{}:{}", uid, data.claims.exp);
            let mut conn = match state.redis_cache.get_conn().await {
                Ok(c) => c,
                Err(_) => {
                    // Redis 故障不应阻塞正常业务,放行(降级策略)
                    // 严格场景下应返回 500
                    return Err(CustomError::internal(String::from("Redis 不可用")));
                }
            };
            use redis::AsyncCommands;
            let is_revoked: Option<String> = conn.get(&blacklist_key).await.unwrap_or(None);
            if is_revoked.is_some() {
                return Err(CustomError::unauthorized("token 已被撤销"));
            }

            // 从缓存或数据库获取用户信息
            let mut public: Option<UserPublic> =
                redis_cache.get_user_public(&uid).await.ok().flatten();
            if public.is_none() {
                let db = &state.db_pool;
                // query_as! 宏在编译期校验 SQL 与 UserRecord 的字段类型/可空性。
                // 三个 NOT NULL 列被映射为 Option<T> 字段,需要 "col!: Option<T>" 强制类型覆盖。
                // 三个 Postgres enum(role/gender/login_method)被 ::text 强转再用 "col: T" 覆盖类型。
                // group_id 子查询需要 "col!: Option<i64>" 给出可空 BIGINT 的提示。
                // u.status 不在 UserRecord 里,从 SELECT 中省略。
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
                              JOIN association_groups g ON g.group_id=agm.group_id AND g.status='ACTIVE'
                              WHERE agm.user_id=u.user_id
                              ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS "group_id!: Option<i64>"
                    FROM users u WHERE u.user_id=$1 AND u.status='ACTIVE'
                    "#,
                    uid
                )
                .fetch_one(db)
                .await
                {
                    public = Some(record.into());
                    if let Some(ref p) = public {
                        let _ = redis_cache.set_user_public(p, 3600).await;
                    }
                }
            }

            if let Some(ref p) = public {
                // 插入一个克隆，避免生命周期问题
                req.extensions_mut().insert::<UserPublic>(p.clone());
            }

            Ok(UserToken {
                exp: data.claims.exp,
                user_id: uid,
                user: public.clone(),
            })
        }
    }
}
