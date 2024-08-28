use crate::{errors::CustomError, models::wx_official::TemplateMessage, utils::BAIDU_AK};
use ntex::web::{
    types::Json,
    HttpResponse, Responder,
};
use reqwest::Client;
use serde_json::Value;

use super::auth::{fetch_set_access_token, get_access_token};

// 发送消息 A-->B
pub async fn send_template(data: Json<TemplateMessage>) -> Result<impl Responder, CustomError> {
    fetch_set_access_token().await?;
    let client = Client::new();
    if let Some(val) = get_access_token().await {
        match weather_handler().await {  
            Ok(value) => {
                let json_data = serde_json::json!({
                    "touser": data.push_id,
                    "template_id": data.template_id,
                    "url": "http://weixin.qq.com/download",
                    "topcolor":"#FF0000",
                    "data":{
                        "date":{
                            "value": value.get("result").unwrap().get("forecasts").unwrap().get(0).unwrap().get("date").unwrap(),
                            "color":"#f5f5f5"
                        },
                        "city":{
                            "value": value.get("result").unwrap().get("location").unwrap().get("city").unwrap(),
                            "color":"#173177"
                        },
                        "weather": {
                            "value": value.get("result").unwrap().get("forecasts").unwrap().get(0).unwrap().get("text_day").unwrap(),
                            "color":"#173177"
                        },
                        "low": {
                            "value": value.get("result").unwrap().get("forecasts").unwrap().get(0).unwrap().get("low").unwrap(),
                            "color":"#173177"
                        },
                        "high": {
                            "value": value.get("result").unwrap().get("forecasts").unwrap().get(0).unwrap().get("high").unwrap(),
                            "color":"#173177"
                        },
                        "loveDays": {
                            "value": data.love_days,
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
            },  
            Err(_) => {  
                Err(CustomError::BadRequest("error".to_string()))  
            },  
        }  
    } else {
        Ok(HttpResponse::Created().body("NO access_token"))
    }
    
}

pub async fn weather_handler() -> Result<Value, CustomError> {
    let client = Client::new();
    let url = format!("https://api.map.baidu.com/weather/v1/?district_id=510100&data_type=all&ak={}", BAIDU_AK);
    let res = client.get(url).send().await?;
    let response_text = res.text().await?;
    let data: Result<Value, serde_json::Error> = serde_json::from_str(&response_text);

    data.map_err(Into::into)
}

pub async fn get_weather() -> Result<HttpResponse, CustomError> {  
    match weather_handler().await {  
        Ok(value) => { 
            println!("result--location: {:?}", value.get("result").unwrap().get("location").unwrap().get("city"));
            println!("result--forecasts: {:?}", value.get("result").unwrap().get("forecasts").unwrap().get(0));
            Ok(HttpResponse::Ok().json(&value))  
        },  
        Err(_) => {  
            Err(CustomError::BadRequest("error".to_string()))  
        },  
    }  
} 