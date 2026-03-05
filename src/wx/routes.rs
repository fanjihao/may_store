use std::sync::Arc;

use crypto::digest::Digest;
use crypto::sha1::Sha1;
use ntex::web::{
    types::{Query, State},
    HttpResponse, Responder,
};
use reqwest::Client;
use serde_xml_rs::from_str;

use crate::{
    config::AppState,
    errors::CustomError,
    wx::models::{Offical, WxSubscriptionTemplateOut, Xml},
    wx::service::WxService,
};

#[utoipa::path(
    get,
    path = "/wx/templates",
    tag = "微信",
    summary = "获取订阅消息模板列表",
    responses(
        (status = 200, body = [WxSubscriptionTemplateOut], description = "获取成功")
    )
)]
pub async fn get_templates(state: State<Arc<AppState>>) -> Result<impl Responder, CustomError> {
    let templates = sqlx::query_as!(
        WxSubscriptionTemplateOut,
        "SELECT template_id, wx_template_id, template_code, template_name, description
         FROM wx_subscription_templates
         WHERE is_active = 1
         ORDER BY template_id ASC"
    )
    .fetch_all(&state.db_pool)
    .await?;

    Ok(HttpResponse::Ok().json(&templates))
}

#[utoipa::path(
    get,
    path = "/wx/sign-verify",
    tag = "微信",
    summary = "服务器验证",
    params(Offical),
    responses(
        (status = 200, body = String),
        (status = 400, body = CustomError)
    )
)]
// 服务器验证
pub async fn wx_sign_verify(data: Query<Offical>) -> Result<String, CustomError> {
    let mut hasher = Sha1::new();
    // 获取微信服务器发送过来的数据
    let timestamp = data.timestamp.as_ref().unwrap();
    let nonce = data.nonce.as_ref().unwrap();
    let signature = data.signature.as_ref().unwrap();
    let echostr = data.echostr.as_ref().unwrap();

    // 自定义的token
    let token = "may_store";

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
#[utoipa::path(
    post,
    path = "/wx/sign-verify",
    tag = "微信",
    summary = "接收消息",
    request_body = String,
    responses(
        (status = 200, body = String),
        (status = 400, body = CustomError)
    )
)]
pub async fn wx_offical_received(
    data: String,
    state: State<Arc<AppState>>,
) -> Result<String, CustomError> {
    let db_pool = &state.clone().db_pool;

    WxService::fetch_set_access_token().await?;
    let xml: Xml =
        from_str(&data).map_err(|e| CustomError::BadRequest(format!("XML解析失败: {}", e)))?;
    let mut already_reply = false;
    let from_user_name = xml.from_user_name.unwrap_or_default();
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
        let token = WxService::get_access_token().await.unwrap();
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
    Ok("".to_string())
}
