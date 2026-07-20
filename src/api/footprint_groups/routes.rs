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
use crate::middlewares::require_group::RequireGroup;
use crate::middlewares::target_group::require_active_target_group_member;
use crate::utils::response::ApiResponse;

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/footprint-groups")
            .route(web::get().to(list_footprint_groups))
            .route(web::post().to(create_footprint_group)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/footprint-groups/{footprint_group_id}")
            .route(web::patch().to(update_footprint_group))
            .route(web::delete().to(delete_footprint_group)),
    );
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintGroupOut {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub group_type: i16, // 0=美食 1=约会 2=旅行 3=纪念日 4=其他
    /// 组的全局足迹容量，不是单个 record_group 的列。
    pub max_capacity: i32,
    /// 由 footprints 动态聚合，不是 record_group 的列。
    pub current_count: i32,
    pub status: i16, // 1=ACTIVE 0=DISABLED
    pub create_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateFootprintGroupInput {
    pub group_name: String, // 1-50 字符
    pub group_type: i16,    // 0=美食 1=约会 2=旅行 3=纪念日 4=其他
    /// 兼容旧客户端；分组没有独立容量，此值不会写入 record_group。
    pub max_capacity: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFootprintGroupInput {
    pub group_name: Option<String>,
    pub group_type: Option<i16>,
    /// 兼容旧客户端；分组没有独立容量，此值不会写入 record_group。
    pub max_capacity: Option<i32>,
    pub status: Option<i16>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListGroupsQuery {
    pub status: Option<i16>, // 默认 1 (ACTIVE)
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
    responses(
        (status = 200, description = "获取成功", body = Vec<FootprintGroupOut>)
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_footprint_groups(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    query: Query<ListGroupsQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    require_active_target_group_member(&state.db_pool, token.user_id, group_id).await?;
    let status = query.status.unwrap_or(1);

    let rows = sqlx::query(
        r#"SELECT rg.id, rg.group_id, rg.group_name, rg.group_type, rg.status, rg.create_time,
                  COALESCE(fc.footprint_count, 0)::INT AS footprint_count,
                  COALESCE(ag.footprint_capacity, gc.footprint_capacity, 50)::INT AS capacity
           FROM record_group rg
           JOIN association_groups ag ON ag.group_id = $1
           LEFT JOIN group_configs gc ON gc.group_id = ag.group_id
           LEFT JOIN (
               SELECT record_group_id, COUNT(*)::INT AS footprint_count
               FROM footprints
               WHERE group_id = $1
                 AND record_group_id IS NOT NULL
                 AND status <> 'DELETED'
               GROUP BY record_group_id
           ) fc ON fc.record_group_id = rg.id
           WHERE rg.group_id = $1 AND rg.status = $2 AND rg.is_global = FALSE
           ORDER BY rg.id ASC"#,
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
            max_capacity: r.get("capacity"),
            current_count: r.get("footprint_count"),
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
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    body: Json<CreateFootprintGroupInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    require_active_target_group_member(&state.db_pool, token.user_id, group_id).await?;
    let input = body.into_inner();

    if input.group_name.trim().is_empty() || input.group_name.chars().count() > 50 {
        return Err(CustomError::invalid_parameter("分组名称 1-50 字符"));
    }
    if !(0..=4).contains(&input.group_type) {
        return Err(CustomError::invalid_parameter(
            "group_type 必须为 0-4（美食/约会/旅行/纪念日/其他）",
        ));
    }

    let row = sqlx::query(
        r#"WITH created AS (
               INSERT INTO record_group (group_id, group_name, group_type, status, is_global)
               VALUES ($1, $2, $3, 1, FALSE)
               RETURNING id, group_id, group_name, group_type, status, create_time
           )
           SELECT c.id, c.group_id, c.group_name, c.group_type, c.status, c.create_time,
                  0::INT AS footprint_count,
                  COALESCE(ag.footprint_capacity, gc.footprint_capacity, 50)::INT AS capacity
           FROM created c
           JOIN association_groups ag ON ag.group_id = c.group_id
           LEFT JOIN group_configs gc ON gc.group_id = ag.group_id"#,
    )
    .bind(group_id)
    .bind(input.group_name.trim())
    .bind(input.group_type)
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
        max_capacity: row.get("capacity"),
        current_count: row.get("footprint_count"),
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
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
    body: Json<UpdateFootprintGroupInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    require_active_target_group_member(&state.db_pool, token.user_id, group_id).await?;
    let input = body.into_inner();

    if let Some(name) = input.group_name.as_deref() {
        if name.trim().is_empty() || name.chars().count() > 50 {
            return Err(CustomError::invalid_parameter("分组名称 1-50 字符"));
        }
    }
    if let Some(group_type) = input.group_type {
        if !(0..=4).contains(&group_type) {
            return Err(CustomError::invalid_parameter(
                "group_type 必须为 0-4（美食/约会/旅行/纪念日/其他）",
            ));
        }
    }
    if let Some(status) = input.status {
        if status != 0 && status != 1 {
            return Err(CustomError::invalid_parameter("status 必须为 0 或 1"));
        }
    }

    let row = sqlx::query(
        r#"WITH updated AS (
               UPDATE record_group
               SET group_name = COALESCE($3, group_name),
                   group_type = COALESCE($4, group_type),
                   status = COALESCE($5, status),
                   update_time = NOW()
               WHERE id = $1 AND group_id = $2 AND is_global = FALSE
               RETURNING id, group_id, group_name, group_type, status, create_time
           )
           SELECT u.id, u.group_id, u.group_name, u.group_type, u.status, u.create_time,
                  COALESCE((
                      SELECT COUNT(*)::INT
                      FROM footprints f
                      WHERE f.record_group_id = u.id AND f.status <> 'DELETED'
                  ), 0)::INT AS footprint_count,
                  COALESCE(ag.footprint_capacity, gc.footprint_capacity, 50)::INT AS capacity
           FROM updated u
           JOIN association_groups ag ON ag.group_id = u.group_id
           LEFT JOIN group_configs gc ON gc.group_id = ag.group_id"#,
    )
    .bind(id)
    .bind(group_id)
    .bind(
        input
            .group_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty()),
    )
    .bind(input.group_type)
    .bind(input.status)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::resource_not_found("分组不存在"))?;

    Ok(ApiResponse::success(FootprintGroupOut {
        id: row.get("id"),
        group_id: row.get("group_id"),
        group_name: row.get("group_name"),
        group_type: row.get("group_type"),
        max_capacity: row.get("capacity"),
        current_count: row.get("footprint_count"),
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
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    require_active_target_group_member(&state.db_pool, token.user_id, group_id).await?;

    // 检查是否还有足迹引用
    let in_use: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM footprints WHERE record_group_id = $1 AND status != 'DELETED'",
    )
    .bind(id)
    .fetch_one(&state.db_pool)
    .await?;

    if in_use > 0 {
        return Err(CustomError::food_capacity_exceeded(
            "分组下仍有足迹，无法删除",
        ));
    }

    let rows_affected = sqlx::query("DELETE FROM record_group WHERE id = $1 AND group_id = $2")
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
