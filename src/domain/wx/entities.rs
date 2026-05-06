// 领域层 - 微信实体

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 微信订阅模板输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxSubscriptionTemplateOut {
    pub template_id: i64,
    pub wx_template_id: String,
    pub template_code: String,
    pub template_name: String,
    pub description: Option<String>,
}

/// XML 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Xml {
    pub to_user: String,
    pub from_user: String,
    pub create_time: i64,
    pub msg_type: String,
    pub content: String,
}

/// 微信公众号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offical {
    pub app_id: String,
    pub app_secret: String,
}
