// API - 通知路由
// FSD.latest.md compliant - 系统通知、订单通知、心愿通知、未读数

use ntex::web::{self, types::State, HttpResponse, Responder, ServiceConfig};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::notification_service::NotificationService;
use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置通知路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/notifications")
            .route("/unread-count", web::get().to(get_unread_count))
            .route("/mark-read/{notification_id}", web::post().to(mark_as_read)),
    );
}

// ========== 响应结构 ==========

/// 未读通知数量响应
#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCountResponse {
    pub count: i32,
}

/// 标记已读响应
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkReadResponse {
    pub status: String,
}

// ========== 处理器 ==========

/// 获取未读通知数量
/// GET /api/notifications/unread-count
#[utoipa::path(
    get,
    path = "/api/notifications/unread-count",
    tag = "通知",
    responses(
        (status = 200, description = "获取成功", body = UnreadCountResponse),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_unread_count(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let count = NotificationService::get_unread_count(&state.db_pool, token.user_id).await?;
    Ok(HttpResponse::Ok().json(UnreadCountResponse { count }))
}

/// 标记通知为已读
/// POST /api/notifications/mark-read/{notification_id}
#[utoipa::path(
    post,
    path = "/api/notifications/mark-read/{notification_id}",
    tag = "通知",
    params(
        ("notification_id" = i64, Path, description = "通知ID")
    ),
    responses(
        (status = 200, description = "标记成功", body = MarkReadResponse),
        (status = 401, description = "未登录"),
        (status = 404, description = "通知不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn mark_as_read(
    state: State<Arc<AppState>>,
    token: UserToken,
    notification_id: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let notification_id = notification_id.into_inner();
    NotificationService::mark_as_read(&state.db_pool, token.user_id, notification_id).await?;
    Ok(HttpResponse::Ok().json(MarkReadResponse { status: "ok".to_string() }))
}