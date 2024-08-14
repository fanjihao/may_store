use std::sync::Arc;

use lazy_static::lazy_static;
use tokio::{sync::Mutex, time::{self, Instant}};

use crate::{errors::CustomError, utils::{OFFCIAL_APP_ID, OFFCIAL_APP_SECRET}};


struct Token {
    access_token: Option<String>,
    expiration_time: Instant,
}

lazy_static! {
    static ref GLOBAL_STATE: Arc<Mutex<Token>> = Arc::new(Mutex::new(Token {
        access_token: None,
        expiration_time: Instant::now(),
    }));
}

pub async fn set_access_token(token: String, expiration_duration: time::Duration) {
    let mut state = GLOBAL_STATE.lock().await;
    state.access_token = Some(token);
    state.expiration_time = Instant::now() + expiration_duration;
}

pub async fn get_access_token() -> Option<String> {
    let state = GLOBAL_STATE.lock().await;
    if state.expiration_time > Instant::now() {
        state.access_token.clone()
    } else {
        None
    }
}

pub async fn fetch_set_access_token() -> Result<(), CustomError> {
    if let Some(_access_token) = get_access_token().await {
        Ok(())
    } else {
        let body = reqwest::get(
            "https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid="
                .to_string()
                + OFFCIAL_APP_ID
                + "&secret="
                + OFFCIAL_APP_SECRET,
        )
        .await?
        .text()
        .await?;
        let response_json: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_str(&body);
        match response_json {
            Ok(obj) => {
                let mut token: &str = "";
                if let Some(val) = obj.get("access_token") {
                    if let Some(t) = val.as_str() {
                        token = t;
                    }
                }
                set_access_token(token.to_string(), time::Duration::from_secs(7200));
                Ok(())
            }
            Err(_) => Err(CustomError::BadRequest("access_token 获取失败".to_string())),
        }
    }
}