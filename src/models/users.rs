use crate::{errors::CustomError, utils::TOKEN_SECRET_KEY, AppState};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use serde::{Deserialize, Serialize};
use std::{future::Future, sync::Arc};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Login {
    // 登录结构体
    /// 用户账号
    pub account: Option<String>,
    /// 用户密码
    pub password: Option<String>,
    /// 登录code
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Register {
    // 注册结构体
    pub account: Option<String>,
    pub password: Option<String>,
    pub avatar: Option<String>,
    pub nick_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IsRegister {
    pub user_id: Option<i32>,
    pub role: Option<i32>,
    pub bind_num: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserInfo {
    // 用户信息结构体
    pub user_id: Option<i32>,
    pub nick_name: Option<String>,
    pub account: Option<String>,
    pub password: Option<String>,
    pub avatar: Option<String>,
    pub gender: Option<i32>,
    pub birthday: Option<chrono::NaiveDate>,
    pub role: Option<i32>,
    pub role_change_time: Option<chrono::DateTime<chrono::Utc>>,
    pub love_point: Option<i32>,
    pub token: Option<String>,
    pub phone: Option<String>,
    pub associate_id: Option<i32>,
    // pub encounter_date: Option<chrono::NaiveDate>,
    // pub correlation_avatar: Option<String>,
    // pub correlation_name: Option<String>,
    // 以下暂时空置
    // pub open_id: Option<String>,
    pub push_id: Option<String>,
    pub code: Option<String>,
    // pub session_key: Option<String>,
}

// 9.25 中间件身份验证
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserToken {
    pub exp: Option<i64>, // Expiration（过期时间），表示令牌的有效期，使用 Unix 时间戳表示
    pub user_id: i32,     // Subject（主题），通常是用户的唯一标识
    // 在这里可以添加其他自定义字段
    pub user_info: Option<UserInfo>, // ✅ 存储用户信息
}

impl<E: ErrorRenderer> FromRequest<E> for UserToken {
    type Error = CustomError;
    // type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &HttpRequest,
        _: &mut Payload,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        // 注意：下面两个变量的类型不能出现引用（req），否则就会出现生命周期问题（future）
        // ✅ 提前获取 Arc<AppState>，避免 req 生命周期问题
        let state = req
            .app_state::<Arc<AppState>>()
            .expect("Failed to get AppState")
            .clone();

        let redis_cache = state.redis_cache.clone();

        // Cookies 中的 access token
        let access_token = req.headers().get("Authorization");
        let fut = async move {
            let access_token = match access_token {
                Some(c) => c.to_str(),
                None => return Err(CustomError::AuthFailed("No login authorization".into())),
            };

            let access_token = if let Ok(str) = access_token {
                str.to_string()
            } else {
                String::new()
            };

            // 设置JWT解码参数
            let decoding_key = DecodingKey::from_secret(TOKEN_SECRET_KEY);
            let validation = Validation::new(Algorithm::HS256);
            let token_data = match decode::<UserToken>(&access_token, &decoding_key, &validation) {
                Ok(token_data) => {
                    println!("Decoded Token: {:?}", token_data);
                    token_data
                }
                Err(err) => {
                    println!("Token Decoding Error: {:?}", err);
                    return Err(CustomError::AuthFailed(
                        format!("Failed to decode token: {}", err).into(),
                    ));
                }
            };
            let user_id = token_data.claims.user_id;

            // ✅ 先尝试从 Redis 获取用户信息
            let mut user_info: Option<UserInfo> =
                redis_cache.get_user(&user_id).await.ok().flatten();

            // 如果 Redis 缓存未命中，则查询数据库
            if user_info.is_none() {
                let db_pool = &state.db_pool;
                user_info =
                    sqlx::query_as!(UserInfo, "SELECT * FROM users WHERE user_id = $1", &user_id)
                        .fetch_one(db_pool)
                        .await
                        .ok();

                // ✅ 存入 Redis 缓存（如果查询成功）
                if let Some(ref info) = user_info {
                    if let Err(e) = redis_cache.set_user(info, 3600).await {
                        println!("Failed to cache user info in Redis: {:?}", e);
                    }
                }
            }

            // ✅ 将用户信息存入 `req.extensions_mut()`，便于后续 API 访问
            if let Some(ref info) = user_info {
                req.extensions_mut().insert(info.clone());
            }

            Ok(Self {
                exp: token_data.claims.exp,
                user_id,
                user_info, // ✅ 直接存入 `UserToken`
            })
        };

        Box::pin(fut)
    }
}
