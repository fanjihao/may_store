use ntex::web::{
    types::{Query, State},
    Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::admin_auth::{require_admin_role, AdminRole, AdminToken};
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use crate::utils::response::ApiResponse;

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AdminAuditLogQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub operator_id: Option<i64>,
    pub action_type: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<i64>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuditCursor {
    created_at: chrono::DateTime<chrono::Utc>,
    id: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminAuditLogOut {
    pub id: i64,
    pub operator_id: Option<i64>,
    pub operator_nickname: Option<String>,
    pub operator_type: String,
    pub action_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<i64>,
    pub detail: Option<serde_json::Value>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/api/admin/audit-logs",
    tag = "后台管理",
    params(AdminAuditLogQuery),
    responses((status = 200, body = CursorPage<AdminAuditLogOut>)),
    security(("bearer_auth" = []))
)]
pub async fn list_admin_audit_logs(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    query: Query<AdminAuditLogQuery>,
) -> Result<impl Responder, CustomError> {
    require_admin_role(&admin, &[AdminRole::SuperAdmin])?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let cursor = query
        .cursor
        .as_deref()
        .and_then(decode_cursor::<AuditCursor>);
    let (cursor_at, cursor_id) = cursor
        .map(|cursor| (Some(cursor.created_at), Some(cursor.id)))
        .unwrap_or((None, None));
    let rows = sqlx::query(
        r#"SELECT a.id, a.operator_id, u.nick_name AS operator_nickname,
                  a.operator_type, a.action_type, a.target_type, a.target_id,
                  a.detail, a.ip, a.user_agent, a.created_at
           FROM audit_logs a
           LEFT JOIN users u ON u.user_id = a.operator_id
           WHERE ($1::bigint IS NULL OR a.operator_id = $1)
             AND ($2::text IS NULL OR a.action_type = $2)
             AND ($3::text IS NULL OR a.target_type = $3)
             AND ($4::bigint IS NULL OR a.target_id = $4)
             AND ($5::timestamptz IS NULL OR a.created_at >= $5)
             AND ($6::timestamptz IS NULL OR a.created_at <= $6)
             AND ($7::timestamptz IS NULL OR (a.created_at, a.id) < ($7, $8))
           ORDER BY a.created_at DESC, a.id DESC
           LIMIT $9"#,
    )
    .bind(query.operator_id)
    .bind(query.action_type.as_deref())
    .bind(query.target_type.as_deref())
    .bind(query.target_id)
    .bind(query.start_date.as_ref())
    .bind(query.end_date.as_ref())
    .bind(cursor_at)
    .bind(cursor_id)
    .bind(limit + 1)
    .fetch_all(&state.db_pool)
    .await?;

    let has_more = rows.len() as i64 > limit;
    let items: Vec<AdminAuditLogOut> = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| AdminAuditLogOut {
            id: row.get("id"),
            operator_id: row.try_get("operator_id").ok().flatten(),
            operator_nickname: row.try_get("operator_nickname").ok().flatten(),
            operator_type: row.get("operator_type"),
            action_type: row.get("action_type"),
            target_type: row.try_get("target_type").ok().flatten(),
            target_id: row.try_get("target_id").ok().flatten(),
            detail: row.try_get("detail").ok().flatten(),
            ip: row.try_get("ip").ok().flatten(),
            user_agent: row.try_get("user_agent").ok().flatten(),
            created_at: row.get("created_at"),
        })
        .collect();
    let next_cursor = if has_more {
        items.last().map(|item| {
            encode_cursor(&AuditCursor {
                created_at: item.created_at,
                id: item.id,
            })
        })
    } else {
        None
    };
    Ok(ApiResponse::success(CursorPage {
        items,
        next_cursor,
        has_more,
        total: None,
    }))
}
