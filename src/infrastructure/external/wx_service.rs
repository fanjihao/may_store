// 基础设施层 - 微信服务
// 微信公众平台集成

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::PgPool;

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

/// 获取激活的订阅模板列表
pub async fn get_active_templates(pool: &PgPool) -> Result<Vec<WxSubscriptionTemplateOut>, sqlx::Error> {
    sqlx::query_as!(
        WxSubscriptionTemplateOut,
        r#"SELECT template_id, wx_template_id, template_code, template_name, description
         FROM wx_subscription_templates
         WHERE is_active = 1
         ORDER BY template_id ASC"#
    )
    .fetch_all(pool)
    .await
}

/// 微信签名验证
pub fn verify_signature(token: &str, timestamp: &str, nonce: &str, signature: &str) -> bool {
    use crypto::digest::Digest;
    use crypto::sha1::Sha1;

    let mut hasher = Sha1::new();
    let mut items = vec![token, nonce, timestamp];
    items.sort();
    let input = items.join("");
    hasher.input_str(&input);
    hasher.result_str() == signature
}
