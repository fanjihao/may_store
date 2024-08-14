use crate::{errors::CustomError, models::wx_official::MsgTemplate};
use ntex::web::{
    types::Json,
    HttpResponse, Responder,
};
use reqwest::Client;

use super::auth::{fetch_set_access_token, get_access_token};

// 发送消息 A-->B
pub async fn send_template(data: Json<MsgTemplate>) -> Result<impl Responder, CustomError> {
    fetch_set_access_token().await?;
    let client = Client::new();
    if let Some(val) = get_access_token().await {
        let json_data = serde_json::json!({
            "touser": data.open_id,
            "template_id": data.template_id,
            "url": data.url,
            "topcolor":"#FF0000",
            "data":{
                "target": {
                    "value":data.target,
                    "color":"#173177"
                },
                "award":{
                    "value":data.award,
                    "color":"#173177"
                },
                "time":{
                    "value": chrono::Utc::now(),
                    "color":"#173177"
                },
            }
        });
        let resp = client
            .post(format!(
                "https://api.weixin.qq.com/cgi-bin/message/template/send?access_token={}",
                val
            ))
            .json(&json_data)
            .send()
            .await?;
        let response_text = resp.text().await?;
        println!("Response: {}", response_text);
    
        Ok(HttpResponse::Created().body("发送成功"))
    } else {
        Ok(HttpResponse::Created().body("NO access_token"))
    }
    
}