use std::sync::Arc;

use crate::{
    errors::CustomError,
    models::users::{IsRegister, Login, Register, UserInfo, UserToken},
    utils::{APP_ID, APP_SECRET, TOKEN_SECRET_KEY},
    AppState,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use ntex::web::{
    types::{Json, Query, State},
    Responder,
};
use serde_json::Value;
use sqlx::Row;

pub async fn wx_login(
    user: Json<Login>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let account = user.account.clone().unwrap_or_default();
    let pwd = user.password.clone();
    let code = user.code.clone().unwrap_or_default();

    let row = sqlx::query!("SELECT u.* FROM users u WHERE u.account=$1", account)
        .fetch_one(db_pool)
        .await?;

    if row.password == pwd {
        let login = reqwest::get(
            "https://api.weixin.qq.com/sns/jscode2session?grant_type=authorization_code&appid="
                .to_string()
                + APP_ID
                + "&secret="
                + APP_SECRET
                + "&js_code="
                + &code,
        )
        .await?
        .text()
        .await?;
        let _response_json: Result<Value, serde_json::Error> = serde_json::from_str(&login);

        let user_id = row.user_id; // Replace with the actual user ID
        let expiration_time = chrono::Utc::now().timestamp() + (3600 * 24 * 7); // Expiry in 1 hour

        let claims = UserToken {
            user_id: user_id,
            exp: Some(expiration_time),
            // access_token: "".to_string()
        };
        let encoding_key = EncodingKey::from_secret(TOKEN_SECRET_KEY);
        let token = encode(&Header::default(), &claims, &encoding_key).expect("Token 解析失败");

        let user_new = sqlx::query_as!(
            UserInfo,
            "SELECT u.* FROM users u WHERE u.account=$1",
            account
        )
        .fetch_one(db_pool)
        .await?;

        Ok(Json((user_new, token)))
    } else {
        Err(CustomError::BadRequest("账号或密码错误".to_string()))
    }
}

// 获取用户信息
pub async fn get_user_info(
    _: UserToken,
    state: State<Arc<AppState>>,
    data: Query<UserInfo>,
) -> Result<Json<UserInfo>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let mut user: Option<UserInfo> = None;

    if data.account.is_none() {
        user = Some(
            sqlx::query_as!(
                UserInfo,
                "select u.* from users u where u.user_id= $1",
                data.user_id
            )
            .fetch_one(db_pool)
            .await?,
        );
    } else {
        user = Some(
            sqlx::query_as!(
                UserInfo,
                "select u.* from users u where u.account= $1",
                data.account
            )
            .fetch_one(db_pool)
            .await?,
        );
    }

    match user {
        Some(u) => Ok(Json(u)),
        None => Err(CustomError::BadRequest("获取失败".to_string())),
    }
}

pub async fn is_register(
    user: Query<Register>,
    state: State<Arc<AppState>>,
) -> Result<Json<IsRegister>, CustomError> {
    let db_pool = &state.clone().db_pool;
    let row = sqlx::query!("SELECT * FROM users WHERE account = $1", user.account)
        .fetch_optional(db_pool)
        .await?;
    if let Some(user_row) = row {
        let bind_sum = {
            let result = sqlx::query(
                "SELECT COUNT(s.*) FROM user_ships s WHERE (s.ship_status = 0 OR s.ship_status = 1 ) AND (s.bind_id = $1 OR s.user_id = $1)",
            )
            .bind(user_row.user_id)
            .fetch_one(db_pool)
            .await?;
            let count: i64 = result.get(0);
            count
        };

        return Ok(Json(IsRegister {
            user_id: Some(user_row.user_id),
            role: Some(user_row.role.expect("获取权限失败")),
            bind_num: Some(bind_sum),
        }));
    }

    Ok(Json(IsRegister {
        user_id: None,
        role: None,
        bind_num: None,
    }))
}
