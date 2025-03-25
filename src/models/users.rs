use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use crate::{errors::CustomError, utils::TOKEN_SECRET_KEY};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use std::future::Future;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Login { // 登录结构体
    /// 用户账号
    pub account: Option<String>,
    /// 用户密码
    pub password: Option<String>,
    /// 登录code
    pub code: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Register { // 注册结构体
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
pub struct UserInfo { // 用户信息结构体
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
    pub user_id: i32, // Subject（主题），通常是用户的唯一标识
    // 在这里可以添加其他自定义字段
}

impl<E: ErrorRenderer> FromRequest<E> for UserToken {
    type Error = CustomError;
    // type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> impl Future<Output = Result<Self, Self::Error>> {
        // 注意：下面两个变量的类型不能出现引用（req），否则就会出现生命周期问题（future）
        // let db_pool = Arc::clone(req.app_state::<Arc<AppState>>().unwrap())
        //     .db_pool
        //     .clone();

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
            let token_data = match decode::<UserToken>(
                &access_token,
                &decoding_key,
                &validation,
            ) {
                Ok(token_data) => {
                    println!("Decoded Token: {:?}", token_data);
                    token_data
                }
                Err(err) => {
                    println!("Token Decoding Error: {:?}", err);
                    return Err(CustomError::AuthFailed(format!("Failed to decode token: {}", err).into()));
                }
            };
            let user_id = token_data.claims.user_id;

            Ok(Self {
                // access_token: access_token.to_string(),
                exp: token_data.claims.exp,
                user_id, // 示例：将用户ID存储在结构体中
            })
        };

        Box::pin(fut)
    }
}