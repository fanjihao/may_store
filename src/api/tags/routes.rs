// API - 菜品标签路由
// FSD §24.4 compliant
// 标签属于组内共享（group_id 非空）或全局（group_id 为 NULL）。

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
        web::scope("/api/groups/{group_id}/tags")
            .route("", web::get().to(list_tags))
            .route("", web::post().to(create_tag))
            .route("/{tag_id}", web::patch().to(update_tag))
            .route("/{tag_id}", web::delete().to(delete_tag)),
    );
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagOut {
    pub tag_id: i64,
    pub tag_name: String,
    pub icon: Option<String>,
    pub group_id: Option<i64>,
    pub sort: i32,
    pub food_count: i64,             // 该标签下菜品数（聚合）
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateTagInput {
    pub tag_name: String,              // 1-32 字符
    pub icon: Option<String>,          // URL
    pub sort: Option<i32>,            // 默认 0
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTagInput {
    pub tag_name: Option<String>,
    pub icon: Option<String>,
    pub sort: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListTagsQuery {
    pub keyword: Option<String>,        // 按名称模糊搜索
    pub limit: Option<i64>,            // 默认 50
}

/// 列出组内标签（包含全局标签）
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/tags",
    tag = "菜品标签 (§24.4)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("keyword" = Option<String>, Query),
        ("limit" = Option<i64>, Query)
    ),
    security(("cookie_auth" = []))
)]
pub async fn list_tags(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<i64>,
    query: Query<ListTagsQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let limit = query.limit.unwrap_or(50).min(200);

    let keyword_pattern = query.keyword.as_ref().map(|k| format!("%{}%", k));

    let rows = sqlx::query(
        r#"SELECT t.tag_id, t.tag_name, t.icon, t.group_id, t.sort, t.created_at,
                  COALESCE(fc.cnt, 0) AS food_count
           FROM tags t
           LEFT JOIN (
               SELECT tag_id, COUNT(*) AS cnt
               FROM foods WHERE is_del = 0
               GROUP BY tag_id
           ) fc ON fc.tag_id = t.tag_id
           WHERE (t.group_id = $1 OR t.group_id IS NULL)
             AND ($2::text IS NULL OR t.tag_name ILIKE $2)
           ORDER BY t.sort ASC, t.tag_id ASC
           LIMIT $3"#,
    )
    .bind(group_id)
    .bind(keyword_pattern)
    .bind(limit)
    .fetch_all(&state.db_pool)
    .await?;

    let result: Vec<TagOut> = rows
        .into_iter()
        .map(|r| TagOut {
            tag_id: r.get("tag_id"),
            tag_name: r.get("tag_name"),
            icon: r.get("icon"),
            group_id: r.get("group_id"),
            sort: r.get("sort"),
            food_count: r.get("food_count"),
            created_at: r.get("created_at"),
        })
        .collect();

    Ok(ApiResponse::success(result))
}

/// 创建标签
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/tags",
    tag = "菜品标签 (§24.4)",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = CreateTagInput,
    security(("cookie_auth" = []))
)]
pub async fn create_tag(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<i64>,
    body: Json<CreateTagInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();

    if input.tag_name.is_empty() || input.tag_name.len() > 32 {
        return Err(CustomError::invalid_parameter("标签名称 1-32 字符"));
    }

    // 容量校验
    let capacity: i64 = sqlx::query_scalar(
        "SELECT COALESCE(tag_capacity, 10) FROM group_configs WHERE group_id = $1"
    )
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await?
    .unwrap_or(10);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tags WHERE group_id = $1"
    )
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;

    if count >= capacity {
        return Err(CustomError::food_capacity_exceeded("标签数量已达上限"));
    }

    let row = sqlx::query(
        r#"INSERT INTO tags (tag_name, icon, group_id, sort)
           VALUES ($1, $2, $3, $4)
           RETURNING tag_id, tag_name, icon, group_id, sort, created_at"#,
    )
    .bind(&input.tag_name)
    .bind(&input.icon)
    .bind(group_id)
    .bind(input.sort.unwrap_or(0))
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
            CustomError::idempotency_conflict("标签名已存在")
        }
        _ => CustomError::from(e),
    })?;

    let today_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM foods WHERE tag_id = $1 AND is_del = 0"
    )
    .bind(row.get::<i64, _>("tag_id"))
    .fetch_one(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(TagOut {
        tag_id: row.get("tag_id"),
        tag_name: row.get("tag_name"),
        icon: row.get("icon"),
        group_id: row.get("group_id"),
        sort: row.get("sort"),
        food_count: today_count,
        created_at: row.get("created_at"),
    }))
}

/// 更新标签
#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}/tags/{tag_id}",
    tag = "菜品标签 (§24.4)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("tag_id" = i64, Path, description = "标签 ID")
    ),
    request_body = UpdateTagInput,
    security(("cookie_auth" = []))
)]
pub async fn update_tag(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<(i64, i64)>,
    body: Json<UpdateTagInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, tag_id) = path.into_inner();
    let input = body.into_inner();

    if let Some(name) = &input.tag_name {
        if name.is_empty() || name.len() > 32 {
            return Err(CustomError::invalid_parameter("标签名称 1-32 字符"));
        }
    }

    let row = sqlx::query(
        r#"UPDATE tags
           SET tag_name = COALESCE($3, tag_name),
               icon = COALESCE($4, icon),
               sort = COALESCE($5, sort)
           WHERE tag_id = $1 AND (group_id = $2 OR group_id IS NULL)
           RETURNING tag_id, tag_name, icon, group_id, sort, created_at"#,
    )
    .bind(tag_id)
    .bind(group_id)
    .bind(&input.tag_name)
    .bind(&input.icon)
    .bind(input.sort)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::resource_not_found("标签不存在"))?;

    let today_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM foods WHERE tag_id = $1 AND is_del = 0"
    )
    .bind(tag_id)
    .fetch_one(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(TagOut {
        tag_id: row.get("tag_id"),
        tag_name: row.get("tag_name"),
        icon: row.get("icon"),
        group_id: row.get("group_id"),
        sort: row.get("sort"),
        food_count: today_count,
        created_at: row.get("created_at"),
    }))
}

/// 删除标签
#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/tags/{tag_id}",
    tag = "菜品标签 (§24.4)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("tag_id" = i64, Path, description = "标签 ID")
    ),
    security(("cookie_auth" = []))
)]
pub async fn delete_tag(
    state: State<Arc<AppState>>,
    _token: UserToken,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, tag_id) = path.into_inner();

    // 检查是否仍有菜品引用
    let in_use: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM foods WHERE tag_id = $1 AND is_del = 0"
    )
    .bind(tag_id)
    .fetch_one(&state.db_pool)
    .await?;

    if in_use > 0 {
        return Err(CustomError::food_capacity_exceeded(
            "标签仍有菜品引用，无法删除",
        ));
    }

    let rows_affected = sqlx::query(
        "DELETE FROM tags WHERE tag_id = $1 AND group_id = $2"
    )
    .bind(tag_id)
    .bind(group_id)
    .execute(&state.db_pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(CustomError::resource_not_found("标签不存在"));
    }
    Ok(ApiResponse::success(serde_json::json!({ "deleted": true })))
}
