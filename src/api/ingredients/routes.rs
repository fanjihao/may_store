// API - 食材 CRUD
// FSD §24.5 compliant
// 食材属于组内共享（group_id 非空）。本组成员可增删改。

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

use crate::application::food_service::IngredientService;
use crate::domain::foods::ingredient::{
    BatchIngredientSortInput, IngredientCreateInput, IngredientOut, IngredientUpdateInput,
};
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/ingredients")
            .route(web::get().to(list_ingredients))
            .route(web::post().to(create_ingredient)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/ingredients/sort")
            .route(web::post().to(sort_ingredients)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/ingredients/{ingredient_id}")
            .route(web::get().to(get_ingredient))
            .route(web::patch().to(update_ingredient))
            .route(web::delete().to(delete_ingredient)),
    );
}

// ========== 内部工具 ==========

/// 校验用户是该组的 ACTIVE 成员
async fn ensure_member(
    state: &Arc<AppState>,
    user_id: i64,
    group_id: i64,
) -> Result<(), CustomError> {
    require_active_target_group_member(&state.db_pool, user_id, group_id).await
}

/// 校验食材存在并属于指定组（不暴露存在性 → 任何"非本组 ID"都返回 404）
async fn ensure_ingredient_in_group(
    state: &Arc<AppState>,
    ingredient_id: i64,
    group_id: i64,
) -> Result<(), CustomError> {
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ingredients WHERE ingredient_id = $1 AND group_id = $2)",
    )
    .bind(ingredient_id)
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;
    if !ok {
        return Err(CustomError::food_not_found("食材不存在"));
    }
    Ok(())
}

/// 校验 name
fn validate_name(name: &str) -> Result<(), CustomError> {
    let trimmed = name.trim();
    let n_chars = trimmed.chars().count();
    if n_chars == 0 || n_chars > 64 {
        return Err(CustomError::invalid_parameter("name 必须 1-64 字符"));
    }
    Ok(())
}

// ========== 5.1 GET /api/groups/{group_id}/ingredients ==========

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct IngredientListQuery {
    pub keyword: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

/// Cursor payload: 编码 (sort, created_at) 二元组
#[derive(Debug, Serialize, Deserialize)]
struct IngredientCursor {
    sort: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/ingredients",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        IngredientListQuery,
    ),
    responses(
        (status = 200, description = "获取成功", body = CursorPage<IngredientOut>),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_ingredients(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    query: Query<IngredientListQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let q = query.into_inner();

    ensure_member(&state, token.user_id, group_id).await?;

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let cursor = q
        .cursor
        .as_deref()
        .and_then(decode_cursor::<IngredientCursor>);
    let (c_sort, c_created): (Option<i32>, Option<chrono::DateTime<chrono::Utc>>) = match &cursor {
        Some(c) => (Some(c.sort), Some(c.created_at)),
        None => (None, None),
    };

    let keyword_pattern = q
        .keyword
        .as_ref()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .map(|k| format!("%{}%", k));

    let rows = sqlx::query(
        r#"SELECT ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at
           FROM ingredients
           WHERE group_id = $1
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::integer IS NULL OR (sort, created_at) > ($3, $4))
           ORDER BY sort ASC, created_at DESC
           LIMIT $5"#,
    )
    .bind(group_id)
    .bind(&keyword_pattern)
    .bind(c_sort)
    .bind(c_created)
    .bind(limit + 1)
    .fetch_all(&state.db_pool)
    .await?;

    let mut items: Vec<IngredientOut> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| IngredientOut {
            id: r.get("ingredient_id"),
            group_id: r.get("group_id"),
            name: r.get("name"),
            unit: r.try_get("unit").ok().flatten(),
            calories: r.try_get("calories").ok().flatten(),
            description: r.try_get("description").ok().flatten(),
            icon: r.try_get("icon").ok().flatten(),
            sort: r.get("sort"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect();

    let has_more = items.len() as i64 > limit;
    if has_more {
        items.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        items.last().map(|last| {
            encode_cursor(&IngredientCursor {
                sort: last.sort,
                created_at: last.created_at,
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

// ========== 5.2 GET /api/groups/{group_id}/ingredients/{ingredient_id} ==========

#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/ingredients/{ingredient_id}",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("ingredient_id" = i64, Path, description = "食材 ID"),
    ),
    responses(
        (status = 200, description = "获取成功", body = IngredientOut),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "食材不存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, ingredient_id) = path.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;
    ensure_ingredient_in_group(&state, ingredient_id, group_id).await?;

    let r = sqlx::query(
        r#"SELECT ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at
           FROM ingredients WHERE ingredient_id = $1 AND group_id = $2"#,
    )
    .bind(ingredient_id)
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(IngredientOut {
        id: r.get("ingredient_id"),
        group_id: r.get("group_id"),
        name: r.get("name"),
        unit: r.try_get("unit").ok().flatten(),
        calories: r.try_get("calories").ok().flatten(),
        description: r.try_get("description").ok().flatten(),
        icon: r.try_get("icon").ok().flatten(),
        sort: r.get("sort"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

// ========== 5.3 POST /api/groups/{group_id}/ingredients ==========

#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/ingredients",
    tag = "食材 (§24.5)",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = IngredientCreateInput,
    responses(
        (status = 201, description = "创建成功", body = IngredientOut),
        (status = 400, description = "参数非法"),
        (status = 403, description = "无权访问该组"),
        (status = 409, description = "同名食材已存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    body: Json<IngredientCreateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;
    validate_name(&input.name)?;

    if let Some(cal) = input.calories {
        if cal < 0 {
            return Err(CustomError::invalid_parameter("calories 不能为负"));
        }
    }

    let row = sqlx::query(
        r#"INSERT INTO ingredients (group_id, name, unit, calories, description, icon)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at"#,
    )
    .bind(group_id)
    .bind(input.name.trim())
    .bind(input.unit.as_deref())
    .bind(input.calories)
    .bind(input.description.as_deref())
    .bind(input.icon.as_deref())
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
            CustomError::idempotency_conflict("同名食材已存在")
        }
        _ => CustomError::from(e),
    })?;

    Ok(ApiResponse::success(IngredientOut {
        id: row.get("ingredient_id"),
        group_id: row.get("group_id"),
        name: row.get("name"),
        unit: row.try_get("unit").ok().flatten(),
        calories: row.try_get("calories").ok().flatten(),
        description: row.try_get("description").ok().flatten(),
        icon: row.try_get("icon").ok().flatten(),
        sort: row.get("sort"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

// ========== 5.4 PATCH /api/groups/{group_id}/ingredients/{ingredient_id} ==========

#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}/ingredients/{ingredient_id}",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("ingredient_id" = i64, Path, description = "食材 ID"),
    ),
    request_body = IngredientUpdateInput,
    responses(
        (status = 200, description = "更新成功", body = IngredientOut),
        (status = 400, description = "参数非法"),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "食材不存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
    body: Json<IngredientUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, ingredient_id) = path.into_inner();
    let input = body.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;
    ensure_ingredient_in_group(&state, ingredient_id, group_id).await?;

    if let Some(ref name) = input.name {
        validate_name(name)?;
    }
    if let Some(cal) = input.calories {
        if cal < 0 {
            return Err(CustomError::invalid_parameter("calories 不能为负"));
        }
    }

    let row = sqlx::query(
        r#"UPDATE ingredients
           SET name = COALESCE($2, name),
               unit = COALESCE($3, unit),
               calories = COALESCE($4, calories),
               description = COALESCE($5, description),
               icon = COALESCE($6, icon),
               updated_at = NOW()
           WHERE ingredient_id = $1 AND group_id = $7
           RETURNING ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at"#,
    )
    .bind(ingredient_id)
    .bind(input.name.as_deref().map(|s| s.trim().to_string()))
    .bind(input.unit.as_deref())
    .bind(input.calories)
    .bind(input.description.as_deref())
    .bind(input.icon.as_deref())
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::food_not_found("食材不存在"))?;

    Ok(ApiResponse::success(IngredientOut {
        id: row.get("ingredient_id"),
        group_id: row.get("group_id"),
        name: row.get("name"),
        unit: row.try_get("unit").ok().flatten(),
        calories: row.try_get("calories").ok().flatten(),
        description: row.try_get("description").ok().flatten(),
        icon: row.try_get("icon").ok().flatten(),
        sort: row.get("sort"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

// ========== 5.5 DELETE /api/groups/{group_id}/ingredients/{ingredient_id} ==========

#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/ingredients/{ingredient_id}",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("ingredient_id" = i64, Path, description = "食材 ID"),
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "食材不存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, ingredient_id) = path.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;

    let rows_affected =
        sqlx::query("DELETE FROM ingredients WHERE ingredient_id = $1 AND group_id = $2")
            .bind(ingredient_id)
            .bind(group_id)
            .execute(&state.db_pool)
            .await?
            .rows_affected();

    if rows_affected == 0 {
        return Err(CustomError::food_not_found("食材不存在"));
    }
    Ok(ApiResponse::success(serde_json::json!({ "deleted": true })))
}

// ========== 5.6 POST /api/groups/{group_id}/ingredients/sort ==========

#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/ingredients/sort",
    tag = "食材 (§24.5)",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = BatchIngredientSortInput,
    responses(
        (status = 200, description = "排序成功"),
        (status = 403, description = "无权访问该组"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn sort_ingredients(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    body: Json<BatchIngredientSortInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;

    // 限制只能排本组的食材（防止跨组 ID 注入）
    if !input.items.is_empty() {
        let ids: Vec<i64> = input.items.iter().map(|i| i.ingredient_id).collect();
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM ingredients
               WHERE group_id = $1 AND ingredient_id = ANY($2)"#,
        )
        .bind(group_id)
        .bind(&ids)
        .fetch_one(&state.db_pool)
        .await?;
        if (count as usize) != input.items.len() {
            return Err(CustomError::food_not_found("部分食材不属于本组"));
        }
    }

    IngredientService::update_ingredients_sort(&state.db_pool, &input).await?;
    Ok(ApiResponse::success(
        serde_json::json!({ "updated": input.items.len() }),
    ))
}
