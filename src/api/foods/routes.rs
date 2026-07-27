// API - 菜品 CRUD
// FSD §5 / API doc part1 §5

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
use crate::middlewares::idempotency::{self, IdempotencyKey, ReservationOutcome};
use crate::middlewares::require_group::RequireGroup;
use crate::middlewares::target_group::require_active_target_group_member;
use crate::utils::response::ApiResponse;

pub fn configure(cfg: &mut ServiceConfig) {
    // 不用 web::scope —— 避免圈住 /api/groups/{id}/foods/{food_id}/mark 等
    cfg.service(
        web::resource("/api/groups/{group_id}/foods")
            .route(web::post().to(create_food))
            .route(web::get().to(list_foods)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/foods/{food_id}")
            .route(web::get().to(get_food))
            .route(web::patch().to(update_food))
            .route(web::delete().to(delete_food)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/foods/{food_id}/hide")
            .route(web::post().to(hide_food)),
    );
}

// ========== 请求/响应结构 ==========

/// 菜品图片项 (FSD §5.1)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodImage {
    pub url: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// 配料项 (FSD §5.1)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodIngredient {
    pub name: String,
    pub amount: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// 步骤项 (FSD §5.1)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodStep {
    pub order: i32,
    pub content: String,
}

/// 创建菜品输入 (FSD §5.1)
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodCreateInput {
    pub name: String,
    pub description: Option<String>,
    pub images: Option<Vec<FoodImage>>,
    /// 单选标签,必填
    pub tag_id: i64,
    pub ingredients: Option<Vec<FoodIngredient>>,
    pub steps: Option<Vec<FoodStep>>,
    pub idempotency_key: Option<String>,
}

/// 更新菜品输入 (FSD §5.4) —— 全部字段可选 (PATCH 语义)
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodUpdateInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub images: Option<Vec<FoodImage>>,
    /// 修改单选标签
    pub tag_id: Option<i64>,
    pub ingredients: Option<Vec<FoodIngredient>>,
    pub steps: Option<Vec<FoodStep>>,
}

/// 隐藏/恢复输入 (FSD §5.6)
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodHideInput {
    pub hidden: bool,
}

/// 列表查询参数 (FSD §5.2)
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct FoodListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<String>, // ACTIVE / HIDDEN / DELETED
    /// 按单个 tag_id 筛选
    pub tag_id: Option<i64>,
    /// 按菜品名/描述模糊搜索
    pub keyword: Option<String>,
    /// 仅返回当前用户点过 LIKE 的菜（"我的最爱"过滤）；
    /// 传 true 时强制 status=ACTIVE，与 tag_id/keyword 是 AND 关系
    pub is_favorite: Option<bool>,
}

/// 标签引用(挂在菜品上,只带最常用的展示字段)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagRef {
    pub tag_id: i64,
    pub name: String,
    pub icon: Option<String>,
}

/// 菜品详情响应 (FSD §5.1 / §5.3)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodDetail {
    pub food_id: i64,
    pub group_id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub images: Vec<FoodImage>,
    /// 单选标签(必填,但保留 Option 以防历史脏数据导致 NULL)
    pub tag: Option<TagRef>,
    pub ingredients: Vec<FoodIngredient>,
    pub steps: Vec<FoodStep>,
    pub status: String, // ACTIVE / HIDDEN / DELETED / AUDITING / REJECTED
    pub created_by: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// 最近一次被下单时间（可能为 NULL，表示从未被下单）
    pub last_order_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 最近一次被确认完成时间（可能为 NULL）
    pub last_completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 当前查看者是否给这个菜点过 LIKE
    pub is_favorited: bool,
}

/// 列表响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodListResponse {
    pub foods: Vec<FoodSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// 列表项(精简,不含 ingredients/steps)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodSummary {
    pub food_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub images: Vec<FoodImage>,
    pub tag: Option<TagRef>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 最近一次被下单时间（可能为 NULL，表示从未被下单）
    pub last_order_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 最近一次被确认完成时间（可能为 NULL）
    pub last_completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 当前查看者是否给这个菜点过 LIKE
    pub is_favorited: bool,
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

/// 把 DB 的 food_status + is_del 映射为 API 层 status 字符串
fn map_status_to_api(food_status: &str, is_del: i16) -> String {
    if is_del == 1 {
        return "DELETED".to_string();
    }
    match food_status {
        "NORMAL" => "ACTIVE",
        "OFF" => "HIDDEN",
        other => other, // AUDITING / REJECTED 直透
    }
    .to_string()
}

/// 把 DB 行映射为 FoodDetail
fn row_to_detail(r: &sqlx::postgres::PgRow) -> FoodDetail {
    let images_json: serde_json::Value = r.try_get("images").unwrap_or(serde_json::json!([]));
    let ingredients_text: Option<String> = r.try_get("ingredients").ok();
    let steps_text: Option<String> = r.try_get("steps").ok();
    let food_status: String = r.get("food_status");
    let is_del: i16 = r.get("is_del");

    // tag 由 JOIN tags 表得出(tag_id / tag_name / tag_icon)
    let tag = r.try_get::<i64, _>("tag_id").ok().map(|tag_id| TagRef {
        tag_id,
        name: r.try_get("tag_name").unwrap_or_default(),
        icon: r.try_get("tag_icon").ok().flatten(),
    });

    FoodDetail {
        food_id: r.get("food_id"),
        group_id: r.try_get("group_id").ok(),
        name: r.get("food_name"),
        description: r.try_get("description").ok(),
        images: serde_json::from_value(images_json).unwrap_or_default(),
        tag,
        ingredients: ingredients_text
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        steps: steps_text
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        status: map_status_to_api(&food_status, is_del),
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        last_order_at: r.try_get("last_order_at").ok().flatten(),
        last_completed_at: r.try_get("last_completed_at").ok().flatten(),
        is_favorited: r.try_get("is_favorited").unwrap_or(false),
    }
}

async fn enrich_ingredient_icons(
    db: &sqlx::PgPool,
    group_id: i64,
    ingredients: &mut [FoodIngredient],
) -> Result<(), CustomError> {
    if ingredients.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = ingredients
        .iter()
        .map(|ingredient| ingredient.name.clone())
        .collect();
    let rows =
        sqlx::query("SELECT name, icon FROM ingredients WHERE group_id = $1 AND name = ANY($2)")
            .bind(group_id)
            .bind(&names)
            .fetch_all(db)
            .await?;
    let icons: std::collections::HashMap<String, Option<String>> = rows
        .into_iter()
        .map(|row| (row.get("name"), row.try_get("icon").ok().flatten()))
        .collect();
    for ingredient in ingredients {
        if let Some(icon) = icons.get(&ingredient.name) {
            ingredient.icon = icon.clone();
        }
    }
    Ok(())
}

// ========== 5.1 POST /api/groups/{group_id}/foods —— 创建菜品 ==========

#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/foods",
    tag = "菜品",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = FoodCreateInput,
    responses(
        (status = 201, description = "创建成功", body = FoodDetail),
        (status = 400, description = "参数非法"),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_food(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    idempotency_key: IdempotencyKey,
    path: Path<i64>,
    body: Json<FoodCreateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();

    ensure_member(&state, token.user_id, group_id).await?;

    // 字段校验
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 50 || name.chars().count() < 2 {
        return Err(CustomError::BadRequest("name 必须 2-50 字符".into()));
    }
    if let Some(ref desc) = input.description {
        if desc.chars().count() > 500 {
            return Err(CustomError::BadRequest("description 最多 500 字".into()));
        }
    }
    if let Some(ref imgs) = input.images {
        if imgs.len() > 9 {
            return Err(CustomError::BadRequest("images 最多 9 张".into()));
        }
    }

    // 校验 tag_id 存在并属于当前组(组内标签 + 全局标签 group_id IS NULL)
    let tag_ok: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
             SELECT 1 FROM tags
             WHERE tag_id = $1 AND (group_id = $2 OR group_id IS NULL)
           )"#,
    )
    .bind(input.tag_id)
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;
    if !tag_ok {
        return Err(CustomError::invalid_parameter("tag_id 不存在或不属于本组"));
    }

    // 同一表单会话使用 body.idempotency_key；旧客户端则退回请求头。
    // 前端连点产生的多个 HTTP 请求因此会复用同一个业务幂等键。
    let request_key = input.idempotency_key.clone().or(idempotency_key.0);
    let route = format!("/api/groups/{group_id}/foods");
    let reservation = match idempotency::reserve(
        &state.redis_cache,
        token.user_id,
        "POST",
        &route,
        request_key.as_deref(),
    )
    .await?
    {
        ReservationOutcome::Bypass => None,
        ReservationOutcome::Acquired(reservation) => Some(reservation),
        ReservationOutcome::Completed(cached) => {
            return Ok(ApiResponse::success(cached.body));
        }
    };

    let create_result: Result<FoodDetail, CustomError> = async {
        let food_id = idgenerator::IdInstance::next_id();
        let images_json = serde_json::to_value(input.images.unwrap_or_default())
            .unwrap_or(serde_json::json!([]));
        let ingredients_text = input
            .ingredients
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        let steps_text = input
            .steps
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        // 默认状态:NORMAL + APPROVED(简化,跳过审核流程);若 FSD §24.7 要走审核,改成 AUDITING + PENDING
        sqlx::query(
            r#"INSERT INTO foods (food_id, food_name, description, images, tag_id, ingredients, steps,
                                  food_status, submit_role, apply_status, created_by, group_id,
                                  created_at, updated_at)
               VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, 'NORMAL'::food_status_enum, 'RECEIVING_CREATE'::submit_role_enum, 'APPROVED'::apply_status_enum, $8, $9, NOW(), NOW())"#,
        )
        .bind(food_id)
        .bind(name)
        .bind(&input.description)
        .bind(&images_json)
        .bind(input.tag_id)
        .bind(&ingredients_text)
        .bind(&steps_text)
        .bind(token.user_id)
        .bind(group_id)
        .execute(&state.db_pool)
        .await?;

        let row = sqlx::query(
            r#"SELECT f.food_id, f.food_name, f.description, f.images, f.ingredients, f.steps,
                      f.tag_id, t.tag_name, t.icon AS tag_icon,
                      f.food_status::text AS food_status, f.is_del, f.created_by, f.group_id, f.created_at, f.updated_at
               FROM foods f LEFT JOIN tags t ON t.tag_id = f.tag_id WHERE f.food_id = $1"#,
        )
        .bind(food_id)
        .fetch_one(&state.db_pool)
        .await?;
        Ok(row_to_detail(&row))
    }
    .await;

    let detail = match create_result {
        Ok(detail) => detail,
        Err(error) => {
            if let Some(reservation) = reservation.as_ref() {
                reservation.release().await;
            }
            return Err(error);
        }
    };
    let payload = serde_json::to_value(detail)
        .map_err(|error| CustomError::internal(format!("菜品响应序列化失败: {error}")))?;
    if let Some(reservation) = reservation.as_ref() {
        reservation.complete(201, &payload).await?;
    }
    Ok(ApiResponse::success(payload))
}

// ========== 5.2 GET /api/groups/{group_id}/foods —— 列表 ==========

#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/foods",
    tag = "菜品",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        FoodListQuery,
    ),
    responses(
        (status = 200, description = "获取成功", body = FoodListResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_foods(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    query: Query<FoodListQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let q = query.into_inner();

    ensure_member(&state, token.user_id, group_id).await?;

    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let after_food_id: Option<i64> = q.cursor.as_deref().and_then(|s| base64_decode_cursor(s));

    // 仅看"我的最爱"时强制 status=ACTIVE（覆盖请求里的 status 参数）；
    // 不传或 false → 按用户传的 status 走原逻辑
    let favorite_only = q.is_favorite.unwrap_or(false);
    let (food_status_filter, include_deleted) = if favorite_only {
        (Some("NORMAL"), false)
    } else {
        let want_status = q.status.as_deref().unwrap_or("ACTIVE");
        match want_status {
            "DELETED" => (None, true),
            "HIDDEN" => (Some("OFF"), false),
            "ACTIVE" => (Some("NORMAL"), false),
            "AUDITING" => (Some("AUDITING"), false),
            "REJECTED" => (Some("REJECTED"), false),
            other => return Err(CustomError::BadRequest(format!("未知 status: {}", other))),
        }
    };

    // keyword 用于 ILIKE 匹配(菜品名 / 描述)
    let keyword_pattern: Option<String> = q
        .keyword
        .as_ref()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .map(|k| format!("%{}%", k));

    // 多取 1 行判 has_more
    // 当 favorite_only 时多拼一个 AND EXISTS 子句，只看我点过 LIKE 的菜
    let mut sql = String::from(
        r#"SELECT f.food_id, f.food_name, f.description, f.images, f.tag_id,
              t.tag_name, t.icon AS tag_icon,
              f.food_status::text AS food_status, f.is_del, f.created_by, f.group_id, f.created_at, f.updated_at,
              lo.last_order_at,
              lo.last_completed_at,
              EXISTS(SELECT 1 FROM user_food_mark ufm
                     WHERE ufm.user_id = $8 AND ufm.food_id = f.food_id AND ufm.mark_type = 'LIKE'::mark_type_enum) AS is_favorited
       FROM foods f
       LEFT JOIN tags t ON t.tag_id = f.tag_id
       LEFT JOIN LATERAL (
         SELECT MAX(o.created_at) AS last_order_at,
                MAX(CASE WHEN o.status = 'CONFIRMED_COMPLETED'::order_status_enum THEN o.updated_at END) AS last_completed_at
         FROM order_items oi
         JOIN orders o ON o.order_id = oi.order_id
         WHERE oi.food_id = f.food_id
       ) lo ON true
       WHERE f.group_id = $1
         AND ($2::food_status_enum IS NULL OR f.food_status = $2::food_status_enum)
         AND f.is_del = $3
         AND ($4::bigint IS NULL OR f.food_id < $4)
         AND ($5::bigint IS NULL OR f.tag_id = $5)
         AND ($6::text IS NULL OR f.food_name ILIKE $6 OR f.description ILIKE $6)"#,
    );
    if favorite_only {
        sql.push_str(
            r#"
         AND EXISTS (
           SELECT 1 FROM user_food_mark ufm_fav
           WHERE ufm_fav.user_id = $8
             AND ufm_fav.food_id = f.food_id
             AND ufm_fav.mark_type = 'LIKE'::mark_type_enum
         )"#,
        );
    }
    sql.push_str(
        r#"
       ORDER BY f.food_id DESC
       LIMIT $7"#,
    );

    let rows = sqlx::query(&sql)
        .bind(group_id)
        .bind(food_status_filter)
        .bind(if include_deleted { 1_i16 } else { 0_i16 })
        .bind(after_food_id)
        .bind(q.tag_id)
        .bind(&keyword_pattern)
        .bind(limit + 1)
        .bind(token.user_id)
        .fetch_all(&state.db_pool)
        .await?;

    let has_more = rows.len() as i64 > limit;
    let take = if has_more { limit as usize } else { rows.len() };

    let foods: Vec<FoodSummary> = rows
        .iter()
        .take(take)
        .map(|r| {
            let images_json: serde_json::Value =
                r.try_get("images").unwrap_or(serde_json::json!([]));
            let food_status: String = r.get("food_status");
            let is_del: i16 = r.get("is_del");
            let tag = r.try_get::<i64, _>("tag_id").ok().map(|tag_id| TagRef {
                tag_id,
                name: r.try_get("tag_name").unwrap_or_default(),
                icon: r.try_get("tag_icon").ok().flatten(),
            });
            FoodSummary {
                food_id: r.get("food_id"),
                name: r.get("food_name"),
                description: r.try_get("description").ok(),
                images: serde_json::from_value(images_json).unwrap_or_default(),
                tag,
                status: map_status_to_api(&food_status, is_del),
                created_at: r.get("created_at"),
                last_order_at: r.try_get("last_order_at").ok().flatten(),
                last_completed_at: r.try_get("last_completed_at").ok().flatten(),
                is_favorited: r.try_get("is_favorited").unwrap_or(false),
            }
        })
        .collect();

    let next_cursor = if has_more {
        foods.last().map(|f| base64_encode_cursor(f.food_id))
    } else {
        None
    };

    Ok(ApiResponse::success(FoodListResponse {
        foods,
        next_cursor,
        has_more,
    }))
}

// 简易游标:直接 base64 编码 food_id (倒序分页基准)
fn base64_encode_cursor(food_id: i64) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(food_id.to_string())
}

fn base64_decode_cursor(s: &str) -> Option<i64> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .ok()?;
    std::str::from_utf8(&bytes).ok()?.parse().ok()
}

// ========== 5.3 GET /api/groups/{group_id}/foods/{food_id} —— 详情 ==========

#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/foods/{food_id}",
    tag = "菜品",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("food_id" = i64, Path, description = "菜品 ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = FoodDetail),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "菜品不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_food(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, food_id) = path.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;

    let row = sqlx::query(
        r#"SELECT f.food_id, f.food_name, f.description, f.images, f.ingredients, f.steps,
                  f.tag_id, t.tag_name, t.icon AS tag_icon,
                  f.food_status::text AS food_status, f.is_del, f.created_by, f.group_id, f.created_at, f.updated_at,
                  lo.last_order_at,
                  lo.last_completed_at,
                  EXISTS(SELECT 1 FROM user_food_mark ufm
                         WHERE ufm.user_id = $3 AND ufm.food_id = f.food_id AND ufm.mark_type = 'LIKE'::mark_type_enum) AS is_favorited
           FROM foods f
           LEFT JOIN tags t ON t.tag_id = f.tag_id
           LEFT JOIN LATERAL (
             SELECT MAX(o.created_at) AS last_order_at,
                    MAX(CASE WHEN o.status = 'CONFIRMED_COMPLETED'::order_status_enum THEN o.updated_at END) AS last_completed_at
             FROM order_items oi
             JOIN orders o ON o.order_id = oi.order_id
             WHERE oi.food_id = f.food_id
           ) lo ON true
           WHERE f.food_id = $1 AND f.group_id = $2"#,
    )
    .bind(food_id)
    .bind(group_id)
    .bind(token.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::food_not_found("菜品不存在"))?;

    let mut detail = row_to_detail(&row);
    enrich_ingredient_icons(
        &state.db_pool,
        detail.group_id.unwrap_or(group_id),
        &mut detail.ingredients,
    )
    .await?;
    Ok(ApiResponse::success(detail))
}

// ========== 5.4 PATCH /api/groups/{group_id}/foods/{food_id} —— 更新 ==========

#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}/foods/{food_id}",
    tag = "菜品",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("food_id" = i64, Path, description = "菜品 ID")
    ),
    request_body = FoodUpdateInput,
    responses(
        (status = 200, description = "更新成功", body = FoodDetail),
        (status = 400, description = "参数非法"),
        (status = 403, description = "仅创建人可编辑"),
        (status = 404, description = "菜品不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_food(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
    body: Json<FoodUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, food_id) = path.into_inner();
    let input = body.into_inner();

    ensure_member(&state, token.user_id, group_id).await?;

    // 检查菜品归属 + 权限(仅创建人或 Seller 角色;此处简化为仅创建人)
    let row: Option<(i64, i16)> =
        sqlx::query_as("SELECT created_by, is_del FROM foods WHERE food_id = $1 AND group_id = $2")
            .bind(food_id)
            .bind(group_id)
            .fetch_optional(&state.db_pool)
            .await?;
    let (created_by, is_del) = row.ok_or_else(|| CustomError::food_not_found("菜品不存在"))?;
    if is_del == 1 {
        return Err(CustomError::food_not_found("菜品已删除"));
    }
    if created_by != token.user_id {
        return Err(CustomError::permission_denied("仅菜品创建人可编辑"));
    }

    // 字段校验(同 create)
    if let Some(ref name) = input.name {
        let n = name.trim();
        if n.chars().count() < 2 || n.chars().count() > 50 {
            return Err(CustomError::BadRequest("name 必须 2-50 字符".into()));
        }
    }
    if let Some(ref imgs) = input.images {
        if imgs.len() > 9 {
            return Err(CustomError::BadRequest("images 最多 9 张".into()));
        }
    }

    // 若要更新 tag_id,校验目标 tag 存在并属于当前组
    if let Some(new_tag_id) = input.tag_id {
        let tag_ok: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                 SELECT 1 FROM tags
                 WHERE tag_id = $1 AND (group_id = $2 OR group_id IS NULL)
               )"#,
        )
        .bind(new_tag_id)
        .bind(group_id)
        .fetch_one(&state.db_pool)
        .await?;
        if !tag_ok {
            return Err(CustomError::invalid_parameter("tag_id 不存在或不属于本组"));
        }
    }

    // 使用 COALESCE 模式:None 则保留原值
    let images_json = input
        .images
        .as_ref()
        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::json!([])));
    let ingredients_text = input
        .ingredients
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let steps_text = input
        .steps
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    sqlx::query(
        r#"UPDATE foods
           SET food_name   = COALESCE($1, food_name),
               description = COALESCE($2, description),
               images      = COALESCE($3::jsonb, images),
               tag_id      = COALESCE($4, tag_id),
               ingredients = COALESCE($5, ingredients),
               steps       = COALESCE($6, steps),
               updated_at  = NOW()
           WHERE food_id = $7 AND group_id = $8"#,
    )
    .bind(input.name.as_deref().map(|s| s.trim().to_string()))
    .bind(&input.description)
    .bind(&images_json)
    .bind(input.tag_id)
    .bind(&ingredients_text)
    .bind(&steps_text)
    .bind(food_id)
    .bind(group_id)
    .execute(&state.db_pool)
    .await?;

    let row = sqlx::query(
        r#"SELECT f.food_id, f.food_name, f.description, f.images, f.ingredients, f.steps,
                  f.tag_id, t.tag_name, t.icon AS tag_icon,
                  f.food_status::text AS food_status, f.is_del, f.created_by, f.group_id, f.created_at, f.updated_at
           FROM foods f LEFT JOIN tags t ON t.tag_id = f.tag_id WHERE f.food_id = $1"#,
    )
    .bind(food_id)
    .fetch_one(&state.db_pool)
    .await?;
    Ok(ApiResponse::success(row_to_detail(&row)))
}

// ========== 5.5 DELETE /api/groups/{group_id}/foods/{food_id} —— 软删除 ==========

#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/foods/{food_id}",
    tag = "菜品",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("food_id" = i64, Path, description = "菜品 ID")
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 403, description = "无权"),
        (status = 404, description = "菜品不存在"),
        (status = 409, description = "存在进行中的订单,不可删除")
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_food(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, food_id) = path.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;

    // 检查归属
    let row: Option<(i64, i16)> =
        sqlx::query_as("SELECT created_by, is_del FROM foods WHERE food_id = $1 AND group_id = $2")
            .bind(food_id)
            .bind(group_id)
            .fetch_optional(&state.db_pool)
            .await?;
    let (created_by, is_del) = row.ok_or_else(|| CustomError::food_not_found("菜品不存在"))?;
    if is_del == 1 {
        return Err(CustomError::food_not_found("菜品已删除"));
    }
    if created_by != token.user_id {
        return Err(CustomError::permission_denied("仅菜品创建人可删除"));
    }

    // FSD §5.5:有进行中订单不可删除
    let has_active_order: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
             SELECT 1 FROM order_items oi
             JOIN orders o ON o.order_id = oi.order_id
             WHERE oi.food_id = $1
               AND o.status NOT IN ('COMPLETED', 'CANCELED', 'REJECTED', 'TIMEOUT')
           )"#,
    )
    .bind(food_id)
    .fetch_one(&state.db_pool)
    .await
    .unwrap_or(false);
    if has_active_order {
        return Err(CustomError::Conflict("存在进行中的订单,不可删除".into()));
    }

    sqlx::query("UPDATE foods SET is_del = 1, updated_at = NOW() WHERE food_id = $1")
        .bind(food_id)
        .execute(&state.db_pool)
        .await?;

    Ok(ApiResponse::success(
        serde_json::json!({ "food_id": food_id }),
    ))
}

// ========== 5.6 POST /api/groups/{group_id}/foods/{food_id}/hide —— 切换隐藏 ==========

#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/foods/{food_id}/hide",
    tag = "菜品",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("food_id" = i64, Path, description = "菜品 ID")
    ),
    request_body = FoodHideInput,
    responses(
        (status = 200, description = "操作成功"),
        (status = 403, description = "无权"),
        (status = 404, description = "菜品不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn hide_food(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
    body: Json<FoodHideInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, food_id) = path.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;

    let row: Option<(i64, i16)> =
        sqlx::query_as("SELECT created_by, is_del FROM foods WHERE food_id = $1 AND group_id = $2")
            .bind(food_id)
            .bind(group_id)
            .fetch_optional(&state.db_pool)
            .await?;
    let (created_by, is_del) = row.ok_or_else(|| CustomError::food_not_found("菜品不存在"))?;
    if is_del == 1 {
        return Err(CustomError::food_not_found("菜品已删除"));
    }
    if created_by != token.user_id {
        return Err(CustomError::permission_denied("仅菜品创建人可隐藏"));
    }

    let new_status = if body.hidden { "OFF" } else { "NORMAL" };
    sqlx::query(
        "UPDATE foods SET food_status = $1::food_status_enum, updated_at = NOW() WHERE food_id = $2",
    )
    .bind(new_status)
    .bind(food_id)
    .execute(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(serde_json::json!({
        "food_id": food_id,
        "status": if body.hidden { "HIDDEN" } else { "ACTIVE" }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_list_query_deserializes_is_favorite_true() {
        let q: FoodListQuery =
            serde_urlencoded::from_str("isFavorite=true&limit=10").expect("must deserialize");
        assert_eq!(q.is_favorite, Some(true));
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn food_list_query_deserializes_is_favorite_false() {
        let q: FoodListQuery =
            serde_urlencoded::from_str("isFavorite=false").expect("must deserialize");
        assert_eq!(q.is_favorite, Some(false));
    }

    #[test]
    fn food_list_query_omits_is_favorite_when_absent() {
        let q: FoodListQuery = serde_urlencoded::from_str("limit=20").expect("must deserialize");
        assert_eq!(q.is_favorite, None);
        assert_eq!(q.limit, Some(20));
    }
}
