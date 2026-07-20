// API - 通知路由
// FSD.latest.md compliant - 系统通知、订单通知、心愿通知、未读数

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::utils::response::ApiResponse;

/// 配置通知路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/notifications")
            .route("", web::get().to(get_notifications))
            .route("/unread-count", web::get().to(get_unread_count))
            .route(
                "/{notification_id}/read",
                web::post().to(mark_single_as_read),
            )
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
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub type_: Option<String>,
}

fn parse_notification_cursor(cursor: Option<&str>) -> Result<Option<i64>, CustomError> {
    cursor
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|_| CustomError::BadRequest("cursor 必须是 i64".into()))
        })
        .transpose()
}

fn parse_notification_type(type_: Option<&str>) -> Result<Option<&str>, CustomError> {
    match type_ {
        Some(value) if matches!(value, "ORDER" | "WISH" | "SIGN_IN" | "SYSTEM") => Ok(Some(value)),
        Some(_) => Err(CustomError::BadRequest(
            "type 必须是 ORDER、WISH、SIGN_IN 或 SYSTEM".into(),
        )),
        None => Ok(None),
    }
}

fn parse_notification_limit(limit: Option<i32>) -> Result<i32, CustomError> {
    let limit = limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(CustomError::BadRequest("limit 必须在 1..100 之间".into()));
    }
    Ok(limit)
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
    security(("bearer_auth" = []))
)]
pub async fn get_notifications(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    query: Query<NotificationQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;
    let limit = parse_notification_limit(query.limit)?;
    let cursor = parse_notification_cursor(query.cursor.as_deref())?;
    let notification_type = parse_notification_type(query.type_.as_deref())?;

    let rows = sqlx::query(
        r#"
        SELECT n.notification_id AS id, n.type::text AS type, n.title, n.content, n.data, n.is_read, n.created_at
        FROM notifications n
        WHERE n.user_id = $1
          AND ($2::boolean IS NULL OR n.is_read = $2)
          AND ($3::notification_type_enum IS NULL OR n.type = $3::notification_type_enum)
          AND ($4::bigint IS NULL OR n.notification_id < $4)
        ORDER BY n.notification_id DESC
        LIMIT $5
        "#,
    )
    .bind(user_id)
    .bind(query.is_read)
    .bind(notification_type)
    .bind(cursor)
    .bind(limit + 1)
    .fetch_all(db)
    .await?;

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

    Ok(ApiResponse::success(NotificationsResponse {
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
    security(("bearer_auth" = []))
)]
pub async fn get_unread_count(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;

    // 获取总未读数
    let total: i32 = sqlx::query_scalar(
        "SELECT COUNT(*)::INT FROM notifications WHERE user_id = $1 AND is_read = false",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    // 获取按类型分类的未读数
    let by_type_rows = sqlx::query(
        r#"
        SELECT type::text AS type, COUNT(*) as count
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

    Ok(ApiResponse::success(UnreadCountResponse {
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
    security(("bearer_auth" = []))
)]
pub async fn mark_single_as_read(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    notification_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let notif_id = notification_id.into_inner();
    let user_id = token.user_id;

    let result = sqlx::query(
        "UPDATE notifications SET is_read = true WHERE notification_id = $1 AND user_id = $2",
    )
    .bind(notif_id)
    .bind(user_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(CustomError::NotFound("通知不存在".into()));
    }

    Ok(ApiResponse::success(MarkReadResponse {
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
    security(("bearer_auth" = []))
)]
pub async fn mark_all_as_read(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
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
                    "UPDATE notifications SET is_read = true WHERE user_id = $1 AND notification_id = ANY($2) AND is_read = false",
                )
                .bind(user_id)
                .bind(ids)
                .execute(db)
                .await?
                .rows_affected() as i32
            }
        } else {
            // 全部标记
            sqlx::query(
                "UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false",
            )
            .bind(user_id)
            .execute(db)
            .await?
            .rows_affected() as i32
        }
    } else {
        // 全部标记
        sqlx::query(
            "UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false",
        )
        .bind(user_id)
        .execute(db)
        .await?
        .rows_affected() as i32
    };

    Ok(ApiResponse::success(MarkAllReadResponse { updated_count }))
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
    security(("bearer_auth" = []))
)]
pub async fn delete_notification(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    notification_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let notif_id = notification_id.into_inner();
    let user_id = token.user_id;

    let result =
        sqlx::query("DELETE FROM notifications WHERE notification_id = $1 AND user_id = $2")
            .bind(notif_id)
            .bind(user_id)
            .execute(db)
            .await?;

    if result.rows_affected() == 0 {
        return Err(CustomError::NotFound("通知不存在".into()));
    }

    Ok(ApiResponse::success(DeleteNotificationResponse {
        status: "ok".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_cursor_requires_i64() {
        assert_eq!(
            parse_notification_cursor(Some("9223372036854775807")).unwrap(),
            Some(i64::MAX)
        );
        assert!(matches!(
            parse_notification_cursor(Some("1 OR 1=1")),
            Err(CustomError::BadRequest(_))
        ));
        assert!(matches!(
            parse_notification_cursor(Some("9223372036854775808")),
            Err(CustomError::BadRequest(_))
        ));
    }

    #[test]
    fn notification_type_uses_strict_whitelist() {
        for value in ["ORDER", "WISH", "SIGN_IN", "SYSTEM"] {
            assert_eq!(parse_notification_type(Some(value)).unwrap(), Some(value));
        }
        for value in ["order", "ALL", "ORDER' OR '1'='1", ""] {
            assert!(matches!(
                parse_notification_type(Some(value)),
                Err(CustomError::BadRequest(_))
            ));
        }
    }

    #[test]
    fn notification_query_accepts_type_parameter_name() {
        let query: NotificationQuery = serde_urlencoded::from_str("type=ORDER").unwrap();
        assert_eq!(query.type_.as_deref(), Some("ORDER"));
    }

    #[test]
    fn notification_limit_stays_within_bounds() {
        assert_eq!(parse_notification_limit(None).unwrap(), 20);
        assert_eq!(parse_notification_limit(Some(1)).unwrap(), 1);
        assert_eq!(parse_notification_limit(Some(100)).unwrap(), 100);
        assert!(matches!(
            parse_notification_limit(Some(0)),
            Err(CustomError::BadRequest(_))
        ));
        assert!(matches!(
            parse_notification_limit(Some(101)),
            Err(CustomError::BadRequest(_))
        ));
    }
}
