// API 层 - 微信路由
// 处理微信公众平台相关的 HTTP 请求

use crypto::digest::Digest;
use crypto::sha1::Sha1;
use ntex::web::{self, types::State, HttpResponse, Responder, ServiceConfig};
use std::sync::Arc;

use crate::{
    config::AppState,
    domain::wx::WxSubscriptionTemplateOut,
    errors::CustomError,
};

/// 配置微信路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/wx")
            .route("/sign-verify", web::get().to(wx_sign_verify))
            .route("/sign-verify", web::post().to(wx_offical_received))
            .route("/templates", web::get().to(get_templates)),
    );
}

pub async fn wx_sign_verify(
    data: web::types::Query<WxVerifyQuery>,
) -> Result<String, CustomError> {
    let mut hasher = Sha1::new();
    let timestamp = data.timestamp.as_deref().ok_or_else(|| CustomError::BadRequest("missing timestamp".into()))?;
    let nonce = data.nonce.as_deref().ok_or_else(|| CustomError::BadRequest("missing nonce".into()))?;
    let signature = data.signature.as_deref().ok_or_else(|| CustomError::BadRequest("missing signature".into()))?;
    let echostr = data.echostr.as_deref().ok_or_else(|| CustomError::BadRequest("missing echostr".into()))?;

    let token = "may_store";
    let mut items = vec![token, nonce, timestamp];
    items.sort();
    let input = items.join("");
    hasher.input_str(&input);
    let result = hasher.result_str();
    if result == signature {
        Ok(echostr.to_string())
    } else {
        Ok("".to_string())
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct WxVerifyQuery {
    pub signature: Option<String>,
    pub timestamp: Option<String>,
    pub nonce: Option<String>,
    pub echostr: Option<String>,
}

pub async fn wx_offical_received(
    _data: String,
    _state: State<Arc<AppState>>,
) -> Result<String, CustomError> {
    Ok("".to_string())
}

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