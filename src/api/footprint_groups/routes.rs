// API - 足迹分组路由
// FSD §24.10 compliant
// 足迹按主题/时间分组（如"周年纪念"、"旅行回忆"）

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
use crate::utils::response::ApiResponse;

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups/{group_id}/footprint-groups")
            .route("", web::get().to(list_footprint_groups))
            .route("", web::post().to(create_footprint_group))
            .route("/{footprint_group_id}", web::patch().to(update_footprint_group))
            .route("/{footprint_group_id}", web::delete().to(delete_footprint_group)),
    );
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintGroupOut {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub group_type: i16,              // 1=主题 2=时间
    pub max_capacity: i32,
    pub current_count: i32,
    pub status: i16,                 // 1=ACTIVE 0=DISABLED
    pub create_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateFootprintGroupInput {
    pub group_name: String,           // 1-50 字符
    pub group_type: i16,              // 1=主题 2=时间
    pub max_capacity: Option<i32>,    // 默认 50
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFootprintGroupInput {
    pub group_name: Option<String>,
    pub group_type: Option<i16>,
    pub max_capacity: Option<i32>,
    pub status: Option<i16>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListGroupsQuery {
    pub status: Option<i16>,           // 默认 1 (ACTIVE)
}

/// 列出足迹分组
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/footprint-groups",
    tag = "足迹分组 (§24.10)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("status" = Option<i16>, Query, description = "1=ACTIVE 0=DISABLED")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_footprint_groups(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<i64>,
    query: Query<ListGroupsQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let status = query.status.unwrap_or(1);

    let rows = sqlx::query(
        r#"SELECT id, group_id, group_name, group_type, max_capacity, current_count, status, create_time
           FROM record_group
           WHERE group_id = $1 AND status = $2
           ORDER BY id ASC"#,
    )
    .bind(group_id)
    .bind(status)
    .fetch_all(&state.db_pool)
    .await?;

    let result: Vec<FootprintGroupOut> = rows
        .into_iter()
        .map(|r| FootprintGroupOut {
            id: r.get("id"),
            group_id: r.get("group_id"),
            group_name: r.get("group_name"),
            group_type: r.get("group_type"),
            max_capacity: r.get("max_capacity"),
            current_count: r.get("current_count"),
            status: r.get("status"),
            create_time: r.get("create_time"),
        })
        .collect();

    Ok(ApiResponse::success(result))
}

/// 创建足迹分组
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/footprint-groups",
    tag = "足迹分组 (§24.10)",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = CreateFootprintGroupInput,
    security(("bearer_auth" = []))
)]
pub async fn create_footprint_group(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<i64>,
    body: Json<CreateFootprintGroupInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();

    if input.group_name.is_empty() || input.group_name.len() > 50 {
        return Err(CustomError::invalid_parameter("分组名称 1-50 字符"));
    }
    if input.group_type != 1 && input.group_type != 2 {
        return Err(CustomError::invalid_parameter("group_type 必须为 1 (主题) 或 2 (时间)"));
    }

    let row = sqlx::query(
        r#"INSERT INTO record_group (group_id, group_name, group_type, max_capacity, current_count, status)
           VALUES ($1, $2, $3, $4, 0, 1)
           RETURNING id, group_id, group_name, group_type, max_capacity, current_count, status, create_time"#,
    )
    .bind(group_id)
    .bind(&input.group_name)
    .bind(input.group_type)
    .bind(input.max_capacity.unwrap_or(50))
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
            CustomError::idempotency_conflict("同组内已存在同名分组")
        }
        _ => CustomError::from(e),
    })?;

    Ok(ApiResponse::success(FootprintGroupOut {
        id: row.get("id"),
        group_id: row.get("group_id"),
        group_name: row.get("group_name"),
        group_type: row.get("group_type"),
        max_capacity: row.get("max_capacity"),
        current_count: row.get("current_count"),
        status: row.get("status"),
        create_time: row.get("create_time"),
    }))
}

/// 更新足迹分组
#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}/footprint-groups/{footprint_group_id}",
    tag = "足迹分组 (§24.10)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("footprint_group_id" = i64, Path, description = "分组 ID")
    ),
    request_body = UpdateFootprintGroupInput,
    security(("bearer_auth" = []))
)]
pub async fn update_footprint_group(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<(i64, i64)>,
    body: Json<UpdateFootprintGroupInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    let input = body.into_inner();

    let row = sqlx::query(
        r#"UPDATE record_group
           SET group_name = COALESCE($3, group_name),
               group_type = COALESCE($4, group_type),
               max_capacity = COALESCE($5, max_capacity),
               status = COALESCE($6, status)
           WHERE id = $1 AND group_id = $2
           RETURNING id, group_id, group_name, group_type, max_capacity, current_count, status, create_time"#,
    )
    .bind(id)
    .bind(group_id)
    .bind(&input.group_name)
    .bind(input.group_type)
    .bind(input.max_capacity)
    .bind(input.status)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::resource_not_found("分组不存在"))?;

    Ok(ApiResponse::success(FootprintGroupOut {
        id: row.get("id"),
        group_id: row.get("group_id"),
        group_name: row.get("group_name"),
        group_type: row.get("group_type"),
        max_capacity: row.get("max_capacity"),
        current_count: row.get("current_count"),
        status: row.get("status"),
        create_time: row.get("create_time"),
    }))
}

/// 删除足迹分组
#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/footprint-groups/{footprint_group_id}",
    tag = "足迹分组 (§24.10)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("footprint_group_id" = i64, Path, description = "分组 ID")
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_footprint_group(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();

    // 检查是否还有足迹引用
    let in_use: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM footprints WHERE record_group_id = $1 AND status != 'DELETED'"
    )
    .bind(id)
    .fetch_one(&state.db_pool)
    .await?;

    if in_use > 0 {
        return Err(CustomError::food_capacity_exceeded(
            "分组下仍有足迹，无法删除",
        ));
    }

    let rows_affected = sqlx::query(
        "DELETE FROM record_group WHERE id = $1 AND group_id = $2"
    )
    .bind(id)
    .bind(group_id)
    .execute(&state.db_pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(CustomError::resource_not_found("分组不存在"));
    }
    Ok(ApiResponse::success(serde_json::json!({ "deleted": true })))
}
