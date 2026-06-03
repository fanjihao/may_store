// API - 通知路由
// FSD.latest.md compliant - 系统通知、订单通知、心愿通知、未读数

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置通知路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/notifications")
            .route("", web::get().to(get_notifications))
            .route("/unread-count", web::get().to(get_unread_count))
            .route("/{notification_id}/read", web::post().to(mark_single_as_read))
            .route("/read-all", web::post().to(mark_all_as_read))
            .route("/{notification_id}", web::delete().to(delete_notification)),
    );
}

// ========== 响应结构 ==========

/// 通知项
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationItem {
    pub id: i64,
    #[serde(rename = "type")]
    pub type_: String,
    pub title: String,
    pub content: String,
    pub data: Option<serde_json::Value>,
    pub is_read: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 通知列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationsResponse {
    pub notifications: Vec<NotificationItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// 未读通知数量响应
#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCountResponse {
    pub total: i32,
    pub by_type: Option<serde_json::Value>,
}

/// 标记已读响应
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkReadResponse {
    pub status: String,
}

/// 批量标记已读响应
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkAllReadResponse {
    pub updated_count: i32,
}

/// 删除通知响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteNotificationResponse {
    pub status: String,
}

// ========== 处理器 ==========

/// 通知查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct NotificationQuery {
    pub cursor: Option<String>,
    pub limit: Option<i32>,
    pub is_read: Option<bool>,
    #[param(rename = "type")]
    pub type_: Option<String>,
}

/// 获取通知列表
/// GET /api/notifications
/// FSD v2: 支持 cursor 分页、is_read 筛选、type 筛选
#[utoipa::path(
    get,
    path = "/api/notifications",
    tag = "通知",
    params(NotificationQuery),
    responses(
        (status = 200, description = "获取成功", body = NotificationsResponse),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_notifications(
    state: State<Arc<AppState>>,
    token: UserToken,
    query: Query<NotificationQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;
    let limit = query.limit.unwrap_or(20).min(100);

    // 构建筛选条件
    let is_read_filter = if let Some(is_read) = query.is_read {
        if is_read {
            "AND n.is_read = true".to_string()
        } else {
            "AND n.is_read = false".to_string()
        }
    } else {
        String::new()
    };

    let type_filter = if let Some(ref notif_type) = query.type_ {
        format!("AND n.type = '{}'", notif_type)
    } else {
        String::new()
    };

    let cursor_filter = if let Some(ref cursor) = query.cursor {
        format!("AND n.id < {}", cursor)
    } else {
        String::new()
    };

    let sql = format!(
        r#"
        SELECT n.id, n.type, n.title, n.content, n.data, n.is_read, n.created_at
        FROM notifications n
        WHERE n.user_id = $1 {} {} {}
        ORDER BY n.created_at DESC
        LIMIT $2
        "#,
        is_read_filter, type_filter, cursor_filter
    );

    let rows = sqlx::query(&sql).bind(user_id).bind(limit + 1).fetch_all(db).await?;

    let has_more = rows.len() > limit as usize;
    let notifications: Vec<NotificationItem> = rows
        .iter()
        .take(limit as usize)
        .map(|r| {
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            NotificationItem {
                id: r.get("id"),
                type_: r.get("type"),
                title: r.get("title"),
                content: r.get("content"),
                data: r.get("data"),
                is_read: r.get("is_read"),
                created_at,
            }
        })
        .collect();

    let next_cursor = if has_more {
        notifications.last().map(|n| n.id.to_string())
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(&NotificationsResponse {
        notifications,
        next_cursor,
        has_more,
    }))
}

/// 获取未读通知数量
/// GET /api/notifications/unread-count
/// FSD v2: 返回 total 和 by_type 分类统计
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
    let db = &state.db_pool;
    let user_id = token.user_id;

    // 获取总未读数
    let total: i32 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false")
            .bind(user_id)
            .fetch_one(db)
            .await?;

    // 获取按类型分类的未读数
    let by_type_rows = sqlx::query(
        r#"
        SELECT type, COUNT(*) as count
        FROM notifications
        WHERE user_id = $1 AND is_read = false
        GROUP BY type
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let mut by_type_map = serde_json::Map::new();
    for row in by_type_rows {
        let notif_type: String = row.get("type");
        let count: i64 = row.get("count");
        by_type_map.insert(notif_type, serde_json::Value::Number(count.into()));
    }

    Ok(HttpResponse::Ok().json(&UnreadCountResponse {
        total,
        by_type: Some(serde_json::Value::Object(by_type_map)),
    }))
}

/// 标记单条通知为已读
/// POST /api/notifications/{notification_id}/read
/// FSD v2
#[utoipa::path(
    post,
    path = "/api/notifications/{notification_id}/read",
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
pub async fn mark_single_as_read(
    state: State<Arc<AppState>>,
    token: UserToken,
    notification_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let notif_id = notification_id.into_inner();
    let user_id = token.user_id;

    let result = sqlx::query(
        "UPDATE notifications SET is_read = true WHERE id = $1 AND user_id = $2",
    )
    .bind(notif_id)
    .bind(user_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(CustomError::NotFound("通知不存在".into()));
    }

    Ok(HttpResponse::Ok().json(&MarkReadResponse {
        status: "ok".to_string(),
    }))
}

/// 批量标记通知为已读
/// POST /api/notifications/read-all
/// FSD v2: notification_ids 不传则全部标记
#[utoipa::path(
    post,
    path = "/api/notifications/read-all",
    tag = "通知",
    request_body = Option<BatchMarkReadRequest>,
    responses(
        (status = 200, description = "标记成功", body = MarkAllReadResponse),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn mark_all_as_read(
    state: State<Arc<AppState>>,
    token: UserToken,
    body: Option<Json<BatchMarkReadRequest>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;

    let updated_count = if let Some(req) = body {
        if let Some(ref ids) = req.notification_ids {
            if ids.is_empty() {
                // 全部标记
                sqlx::query("UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false")
                    .bind(user_id)
                    .execute(db)
                    .await?
                    .rows_affected() as i32
            } else {
                // 标记指定通知
                sqlx::query(
                    "UPDATE notifications SET is_read = true WHERE user_id = $1 AND id = ANY($2) AND is_read = false",
                )
                .bind(user_id)
                .bind(ids)
                .execute(db)
                .await?
                .rows_affected() as i32
            }
        } else {
            // 全部标记
            sqlx::query("UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false")
                .bind(user_id)
                .execute(db)
                .await?
                .rows_affected() as i32
        }
    } else {
        // 全部标记
        sqlx::query("UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false")
            .bind(user_id)
            .execute(db)
            .await?
            .rows_affected() as i32
    };

    Ok(HttpResponse::Ok().json(&MarkAllReadResponse { updated_count }))
}

/// 批量标记已读请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct BatchMarkReadRequest {
    pub notification_ids: Option<Vec<i64>>,
}

/// 删除通知
/// DELETE /api/notifications/{notification_id}
/// FSD v2
#[utoipa::path(
    delete,
    path = "/api/notifications/{notification_id}",
    tag = "通知",
    params(
        ("notification_id" = i64, Path, description = "通知ID")
    ),
    responses(
        (status = 200, description = "删除成功", body = DeleteNotificationResponse),
        (status = 401, description = "未登录"),
        (status = 404, description = "通知不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn delete_notification(
    state: State<Arc<AppState>>,
    token: UserToken,
    notification_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let notif_id = notification_id.into_inner();
    let user_id = token.user_id;

    let result =
        sqlx::query("DELETE FROM notifications WHERE id = $1 AND user_id = $2")
            .bind(notif_id)
            .bind(user_id)
            .execute(db)
            .await?;

    if result.rows_affected() == 0 {
        return Err(CustomError::NotFound("通知不存在".into()));
    }

    Ok(HttpResponse::Ok().json(&DeleteNotificationResponse {
        status: "ok".to_string(),
    }))
}