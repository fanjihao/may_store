use std::sync::Arc;

use ntex::web::{types::State, HttpResponse, Responder};

use crate::{
    errors::CustomError,
    models::wx::WxSubscriptionTemplateOut,
    AppState,
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
