// 基础设施层 - 微信服务
//
// ⚠️ V1.0 范围：仅保留 `verify_signature` 函数（用于七牛回调鉴权）。
//
// FSD §10.5 微信订阅消息 / FSD §24.12 wx_subscription_templates V2.0 暂缓。
// 相关函数（send_subscription_message / get_access_token 等）已移除，
// 待 V2.0 重新设计时再补。

use sqlx::PgPool;
use utoipa::ToSchema;
use serde::{Deserialize, Serialize};

/// 微信订阅模板输出
///
/// V1.0 仅用于后台管理端查询，V2.0 将接入订阅消息发送逻辑。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxSubscriptionTemplateOut {
    pub template_id: i64,
    pub wx_template_id: String,
    pub template_code: String,
    pub template_name: String,
    pub description: Option<String>,
}

/// 获取激活的订阅模板列表（V1.0 仅查询，V2.0 接入发送）
#[allow(dead_code)]
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

/// 微信回调签名验证
///
/// 用于：
/// - 七牛云 upload 异步回调（`POST /api/uploads/qiniu-callback`）
/// - 微信服务器事件回调（V2.0）
///
/// 验证方法（参考微信官方）：
/// 1. 将 token、timestamp、nonce 三个参数按字典序排序
/// 2. 拼接为字符串后做 SHA1
/// 3. 与 signature 比对
#[allow(dead_code)]
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
