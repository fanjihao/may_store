use std::sync::Arc;

use lazy_static::lazy_static;
use reqwest::Client;
use tokio::{
    sync::Mutex,
    time::{self, Instant},
};

use crate::{
    errors::CustomError,
    utils::{APP_ID, APP_SECRET, OFFCIAL_APP_ID, OFFCIAL_APP_SECRET},
};

struct Token {
    access_token: Option<String>,
    expiration_time: Instant,
}

lazy_static! {
    // 公众号 Token
    static ref GLOBAL_STATE: Arc<Mutex<Token>> = Arc::new(Mutex::new(Token {
        access_token: None,
        expiration_time: Instant::now(),
    }));

    // 小程序 Token
    static ref MP_STATE: Arc<Mutex<Token>> = Arc::new(Mutex::new(Token {
        access_token: None,
        expiration_time: Instant::now(),
    }));
}

// ================= 公众号 Token 管理 =================

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
                println!("official token : {}", token);
                set_access_token(token.to_string(), time::Duration::from_secs(7200)).await;
                Ok(())
            }
            Err(_) => Err(CustomError::BadRequest(
                "official access_token 获取失败".to_string(),
            )),
        }
    }
}

// ================= 小程序 Token 管理 =================

pub async fn set_mp_token(token: String, expiration_duration: time::Duration) {
    let mut state = MP_STATE.lock().await;
    state.access_token = Some(token);
    state.expiration_time = Instant::now() + expiration_duration;
}

pub async fn get_mp_token() -> Option<String> {
    let state = MP_STATE.lock().await;
    if state.expiration_time > Instant::now() {
        state.access_token.clone()
    } else {
        None
    }
}

pub async fn fetch_set_mp_token() -> Result<(), CustomError> {
    if let Some(_access_token) = get_mp_token().await {
        Ok(())
    } else {
        let body = reqwest::get(
            "https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid="
                .to_string()
                + APP_ID
                + "&secret="
                + APP_SECRET,
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
                println!("mp token : {}", token);
                set_mp_token(token.to_string(), time::Duration::from_secs(7200)).await;
                Ok(())
            }
            Err(_) => Err(CustomError::BadRequest(
                "mp access_token 获取失败".to_string(),
            )),
        }
    }
}

// 创建菜单
pub async fn wx_offical_create_menu() -> Result<String, CustomError> {
    let client = Client::new();
    let token = get_access_token().await;
    let token = match token {
        Some(token) => token,
        None => {
            fetch_set_access_token().await?;
            let new_token = get_access_token().await.unwrap();
            new_token
        }
    };
    let json_data = serde_json::json!({
        "button":[
            {
                "type":"click",
                "name":"绑定PushId",
                "key":"BIND_PUSH_ID"
            },
            {
                "name":"菜单",
                "sub_button":[
                    {
                        "type":"click",
                        "name":"暂定",
                        "key":"NOW_NOTHING"
                    },
                    {
                        "type":"click",
                        "name":"赞一下我们",
                        "key":"V1001_GOOD"
                    }
                ]
            }
        ]
    });
    client
        .post(format!(
            "https://api.weixin.qq.com/cgi-bin/menu/create?access_token={}",
            token
        ))
        .json(&json_data)
        .send()
        .await?;
    Ok("".to_string())
}
