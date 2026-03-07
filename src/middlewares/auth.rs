use std::{future::Future, sync::Arc};

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use serde::{Deserialize, Serialize};

use crate::config::{AppState, TOKEN_SECRET_KEY};
use crate::errors::CustomError;
use crate::users::models::user::{UserPublic, UserRecord};

// ========== Token Claims ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTokenClaims {
    pub exp: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserToken {
    pub exp: i64,
    pub user_id: i64,
    pub user: Option<UserPublic>,
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

            let decoding_key = DecodingKey::from_secret(TOKEN_SECRET_KEY);
            let validation = Validation::new(Algorithm::HS256);
            let data = decode::<UserTokenClaims>(&raw, &decoding_key, &validation)
                .map_err(|e| CustomError::unauthorized(format!("decode token error: {}", e)))?;
            let uid = data.claims.user_id;

            // 从缓存或数据库获取用户信息
            let mut public: Option<UserPublic> =
                redis_cache.get_user_public(&uid).await.ok().flatten();
            if public.is_none() {
                let db = &state.db_pool;
                if let Ok(record) = sqlx::query_as::<_, UserRecord>(
                    r#"
                    SELECT u.user_id, u.username, u.email, u.nick_name, u.role, u.love_point, u.avatar, u.phone,
                           u.open_id, u.status, u.created_at, u.updated_at, u.password_hash,
                           u.password_algo, u.gender, u.birthday, u.username_change, u.login_method,
                           u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
                           (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
                    FROM users u WHERE u.user_id=$1 AND u.status=1
                    "#
                )
                .bind(uid)
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
