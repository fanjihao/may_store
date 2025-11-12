extern crate crypto;
use std::sync::Arc;

use crypto::digest::Digest;
use crypto::sha1::Sha1;
use ntex::web::types::{Query, State};
use reqwest::Client;
use serde_xml_rs::from_str;

use crate::{
    errors::CustomError,
    models::wx_official::{Offical, Xml},
    wx_official::auth::{fetch_set_access_token, get_access_token},
    AppState,
};

// 服务器验证
pub async fn wx_offical_username(data: Query<Offical>) -> Result<String, CustomError> {
    let mut hasher = Sha1::new();
    // 获取微信服务器发送过来的数据
    let timestamp = data.timestamp.as_ref().unwrap();
    let nonce = data.nonce.as_ref().unwrap();
    let signature = data.signature.as_ref().unwrap();
    let echostr = data.echostr.as_ref().unwrap();

    // 自定义的token
    let token = "maystore";

    // 进行字典序排序
    let mut items = vec![token, nonce, timestamp];
    items.sort();
    let input = items.join("");
    let input = input.as_str();

    // 进行加密
    hasher.input_str(input);
    let result = hasher.result_str();
    if result == signature.to_string() {
        Ok(echostr.to_string())
    } else {
        Ok("".to_string())
    }
}

// 接收消息
pub async fn wx_offical_received(
    data: String,
    state: State<Arc<AppState>>,
) -> Result<String, CustomError> {
    let db_pool = &state.clone().db_pool;

    fetch_set_access_token().await?;
    let xml: Xml = from_str(&data).unwrap();
    let mut already_reply = false;
    let from_user_name = xml.from_user_name.unwrap_or_default();
    // let msg_type = xml.msg_type.unwrap_or_default();
    // let event = xml.event.unwrap_or_default();
    // let event_key = xml.event_key.unwrap_or_default();
    let content = xml.content.unwrap_or_default();

    if content.starts_with("绑定") && !already_reply {
        let username = content.split(" ").skip(1).next();
        let result = match username {
            Some(username) => {
                let sum = sqlx::query!("SELECT COUNT(*) FROM users WHERE username = $1", username,)
                    .fetch_one(db_pool)
                    .await?;

                if sum.count.unwrap() > 0_i64 {
                    sqlx::query!(
                        "UPDATE users SET push_id = $1 WHERE username = $2",
                        from_user_name,
                        username,
                    )
                    .execute(db_pool)
                    .await?;
                    "绑定成功"
                } else {
                    "该账户不存在"
                }
            }
            None => "解析账号失败",
        };

        let client = Client::new();
        let token = get_access_token().await.unwrap();
        let res = client
            .post(format!(
                "https://api.weixin.qq.com/cgi-bin/message/custom/send?access_token={}",
                token
            ))
            .json(&serde_json::json!({
                "touser": from_user_name,
                "msgtype": "text",
                "text": {
                    "content": result
                }
            }))
            .send()
            .await?
            .text()
            .await?;
        let response_json: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_str(&res);
        match response_json {
            Ok(obj) => {
                if let Some(val) = obj.get("errmsg") {
                    if let Some(s) = val.as_str() {
                        already_reply = s == "ok".to_string();
                        println!("response: {:?}", already_reply);
                    }
                }
            }
            Err(_) => (),
        };
    }
    // // 点击菜单事件
    // if msg_type == "event".to_string() && event == "CLICK".to_string() && !already_reply {
    //     if event_key == "BIND_PUSH_ID" {
    //     }
    // }
    Ok("".to_string())
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
