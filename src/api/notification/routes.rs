// API 层 - 通知路由
// 处理系统通知、消息推送等HTTP请求

use ntex::web::{self, types::State, HttpResponse, Responder, ServiceConfig};
use sqlx::Row;
use std::sync::Arc;

use crate::application::notification_service::NotificationService;
use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置通知路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/notifications")
            .route("", web::get().to(get_notifications))
            .route("/unread-count", web::get().to(get_unread_count))
            .route("/read-all", web::post().to(mark_all_as_read)),
    );
}

/// 获取通知列表
#[utoipa::path(
    get,
    path = "/notifications",
    tag = "通知",
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_notifications(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;

    // 获取通知列表
    let notifications = sqlx::query(
        r#"SELECT id, type, title, content, is_read, created_at
           FROM notifications
           WHERE user_id = $1
           ORDER BY created_at DESC
           LIMIT 50"#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let result: Vec<serde_json::Value> = notifications
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "type": r.get::<String, _>("type"),
                "title": r.get::<String, _>("title"),
                "content": r.get::<Option<String>, _>("content"),
                "isRead": r.get::<bool, _>("is_read"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(&result))
}

/// 获取未读通知数量
#[utoipa::path(
    get,
    path = "/notifications/unread-count",
    tag = "通知",
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_unread_count(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;

    let count = NotificationService::get_unread_count(db, user_id).await?;

    Ok(HttpResponse::Ok().json(&serde_json::json!({"unreadCount": count})))
}

/// 标记所有通知为已读
#[utoipa::path(
    post,
    path = "/notifications/read-all",
    tag = "通知",
    responses(
        (status = 200, description = "操作成功"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn mark_all_as_read(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;

    sqlx::query("UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false")
        .bind(user_id)
        .execute(db)
        .await?;

    Ok(HttpResponse::Ok().json(&serde_json::json!({"status": "ok"})))
}
