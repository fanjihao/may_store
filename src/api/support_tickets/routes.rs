// API - 客服工单路由
// FSD §15.3 compliant
// 客户端：用户提交工单、查看自己的工单。
// 管理端：管理员查看/回复工单（路由在 /api/admin 下的 admin 模块）。

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
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use crate::utils::response::ApiResponse;

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/support-tickets")
            .route("", web::post().to(create_ticket))
            .route("", web::get().to(list_my_tickets))
            .route("/{ticket_id}", web::get().to(get_ticket)),
    );
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TicketOut {
    pub ticket_id: i64,
    pub user_id: i64,
    pub group_id: Option<i64>,
    pub order_id: Option<i64>,
    pub wish_id: Option<i64>,
    pub category: String,             // BUG / COMPLAINT / SUGGESTION / OTHER
    pub content: String,
    pub images: Option<serde_json::Value>,
    pub status: String,              // PENDING / PROCESSING / RESOLVED / CLOSED
    pub handler_id: Option<i64>,
    pub resolution: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateTicketInput {
    pub category: String,             // BUG / COMPLAINT / SUGGESTION / OTHER
    pub content: String,              // 1-2000 字符
    pub images: Option<Vec<String>>,  // 已上传的图片 URL 列表（最多 9 张）
    pub group_id: Option<i64>,        // 关联组（可选）
    pub order_id: Option<i64>,        // 关联订单（可选）
    pub wish_id: Option<i64>,         // 关联心愿（可选）
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListMyTicketsQuery {
    pub cursor: Option<String>,         // 上一页响应里的 next_cursor
    pub status: Option<String>,         // 筛选 PENDING / PROCESSING / RESOLVED / CLOSED
    pub limit: Option<i64>,            // 默认 20
}

/// Cursor payload: 编码 ticket_id（用 ticket_id 单一字段做稳定分页，
/// 因为 created_at DESC 时 ticket_id 升序作为稳定的 tiebreaker）
#[derive(Debug, Serialize, Deserialize)]
struct TicketCursor {
    ticket_id: i64,
}

/// 用户提交工单
#[utoipa::path(
    post,
    path = "/api/support-tickets",
    tag = "客服工单 (§15.3)",
    request_body = CreateTicketInput,
    security(("bearer_auth" = []))
)]
pub async fn create_ticket(
    state: State<Arc<AppState>>,
    token: UserToken,
    body: Json<CreateTicketInput>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();

    if input.content.is_empty() || input.content.len() > 2000 {
        return Err(CustomError::invalid_parameter("工单内容 1-2000 字符"));
    }
    if !["BUG", "COMPLAINT", "SUGGESTION", "OTHER"].contains(&input.category.as_str()) {
        return Err(CustomError::invalid_parameter("category 必须是 BUG/COMPLAINT/SUGGESTION/OTHER"));
    }
    if let Some(imgs) = &input.images {
        if imgs.len() > 9 {
            return Err(CustomError::upload_size_exceeded("工单图片最多 9 张"));
        }
    }

    // 至少需要一个关联（group_id / order_id / wish_id），便于客服定位
    if input.group_id.is_none() && input.order_id.is_none() && input.wish_id.is_none() {
        return Err(CustomError::invalid_parameter("工单必须关联一个组、订单或心愿"));
    }

    // 校验关联资源存在
    if let Some(gid) = input.group_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM association_groups WHERE group_id = $1)"
        )
        .bind(gid)
        .fetch_one(&state.db_pool)
        .await?;
        if !exists {
            return Err(CustomError::group_not_found("关联组不存在"));
        }
    }
    if let Some(oid) = input.order_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM orders WHERE order_id = $1)"
        )
        .bind(oid)
        .fetch_one(&state.db_pool)
        .await?;
        if !exists {
            return Err(CustomError::order_not_found("关联订单不存在"));
        }
    }
    if let Some(wid) = input.wish_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wishes WHERE wish_id = $1)"
        )
        .bind(wid)
        .fetch_one(&state.db_pool)
        .await?;
        if !exists {
            return Err(CustomError::wish_not_found("关联心愿不存在"));
        }
    }

    let images_json = input.images.as_ref().map(|imgs| serde_json::json!(imgs));

    let row = sqlx::query(
        r#"INSERT INTO support_tickets
            (user_id, group_id, order_id, wish_id, category, content, images, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING')
           RETURNING ticket_id, user_id, group_id, order_id, wish_id, category, content, images,
                     status, handler_id, resolution, created_at, resolved_at"#,
    )
    .bind(token.user_id)
    .bind(input.group_id)
    .bind(input.order_id)
    .bind(input.wish_id)
    .bind(&input.category)
    .bind(&input.content)
    .bind(images_json)
    .fetch_one(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(row_to_ticket_out(&row)))
}

/// 用户查看自己的工单列表
#[utoipa::path(
    get,
    path = "/api/support-tickets",
    tag = "客服工单 (§15.3)",
    params(
        ("cursor" = Option<String>, Query, description = "上一页响应里的 next_cursor"),
        ("status" = Option<String>, Query),
        ("limit" = Option<i64>, Query)
    ),
    responses(
        (status = 200, description = "成功", body = CursorPage<TicketOut>),
        (status = 401, description = "未登录")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_my_tickets(
    state: State<Arc<AppState>>,
    token: UserToken,
    query: Query<ListMyTicketsQuery>,
) -> Result<impl Responder, CustomError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let status = query.status.clone();
    let cursor = query.cursor.as_deref().and_then(decode_cursor::<TicketCursor>);
    let c_ticket_id: Option<i64> = cursor.as_ref().map(|c| c.ticket_id);

    let rows = sqlx::query(
        r#"SELECT ticket_id, user_id, group_id, order_id, wish_id, category, content, images,
                  status, handler_id, resolution, created_at, resolved_at
           FROM support_tickets
           WHERE user_id = $1
             AND ($2::TEXT IS NULL OR status = $2)
             AND ($3::BIGINT IS NULL OR ticket_id < $3)
           ORDER BY created_at DESC, ticket_id ASC
           LIMIT $4"#,
    )
    .bind(token.user_id)
    .bind(status)
    .bind(c_ticket_id)
    .bind(limit + 1)
    .fetch_all(&state.db_pool)
    .await?;

    let mut tickets: Vec<TicketOut> = rows.iter().map(row_to_ticket_out).collect();

    let has_more = tickets.len() > limit as usize;
    if has_more {
        tickets.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        tickets.last().map(|t| {
            encode_cursor(&TicketCursor { ticket_id: t.ticket_id })
        })
    } else {
        None
    };

    Ok(ApiResponse::success(CursorPage {
        items: tickets,
        next_cursor,
        has_more,
        total: None,
    }))
}

/// 用户查看工单详情
#[utoipa::path(
    get,
    path = "/api/support-tickets/{ticket_id}",
    tag = "客服工单 (§15.3)",
    params(("ticket_id" = i64, Path, description = "工单 ID")),
    security(("bearer_auth" = []))
)]
pub async fn get_ticket(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let ticket_id = path.into_inner();

    let row = sqlx::query(
        r#"SELECT ticket_id, user_id, group_id, order_id, wish_id, category, content, images,
                  status, handler_id, resolution, created_at, resolved_at
           FROM support_tickets
           WHERE ticket_id = $1"#,
    )
    .bind(ticket_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::resource_not_found("工单不存在"))?;

    // 仅工单创建人可查看（管理员通过 admin 接口查看）
    let owner: i64 = row.get("user_id");
    if owner != token.user_id {
        return Err(CustomError::permission_denied("仅工单创建人可查看"));
    }

    Ok(ApiResponse::success(row_to_ticket_out(&row)))
}

// ========== 辅助函数 ==========

fn row_to_ticket_out(row: &sqlx::postgres::PgRow) -> TicketOut {
    TicketOut {
        ticket_id: row.get("ticket_id"),
        user_id: row.get("user_id"),
        group_id: row.get("group_id"),
        order_id: row.get("order_id"),
        wish_id: row.get("wish_id"),
        category: row.get("category"),
        content: row.get("content"),
        images: row.get("images"),
        status: row.get("status"),
        handler_id: row.get("handler_id"),
        resolution: row.get("resolution"),
        created_at: row.get("created_at"),
        resolved_at: row.get("resolved_at"),
    }
}
