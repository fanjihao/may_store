// API - 菜品标记路由
// FSD §24.6 compliant
// 用户对菜品标记 LIKE / NOT_RECOMMEND

use ntex::web::{
    self,
    types::{Json, Path, State},
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

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/foods/{food_id}/mark")
            .route(web::post().to(mark_food))
            .route(web::delete().to(unmark_food))
            .route(web::get().to(get_food_mark)),
    );
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkType {
    Like,
    NotRecommend,
}

impl MarkType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Like => "LIKE",
            Self::NotRecommend => "NOT_RECOMMEND",
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarkFoodInput {
    pub mark_type: MarkType,           // LIKE / NOT_RECOMMEND
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodMarkOut {
    pub food_id: i64,
    pub user_id: i64,
    pub mark_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 标记菜品（LIKE 或 NOT_RECOMMEND）
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/foods/{food_id}/mark",
    tag = "菜品标记 (§24.6)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("food_id" = i64, Path, description = "菜品 ID")
    ),
    request_body = MarkFoodInput,
    security(("bearer_auth" = []))
)]
pub async fn mark_food(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
    body: Json<MarkFoodInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, food_id) = path.into_inner();
    let input = body.into_inner();

    // 校验菜品存在
    let food_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM foods WHERE food_id = $1 AND (group_id = $2 OR group_id IS NULL))"
    )
    .bind(food_id)
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;

    if !food_exists {
        return Err(CustomError::food_not_found("菜品不存在"));
    }

    // 校验成员
    let member: Option<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM association_group_members WHERE user_id = $1 AND group_id = $2 AND member_status = 'ACTIVE'"
    )
    .bind(token.user_id)
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await?;
    if member.is_none() {
        return Err(CustomError::permission_denied("不是该组成员"));
    }

    // 删除旧标记（同用户同菜品只能一个标记）
    sqlx::query("DELETE FROM user_food_mark WHERE user_id = $1 AND food_id = $2")
        .bind(token.user_id)
        .bind(food_id)
        .execute(&state.db_pool)
        .await?;

    // 插入新标记
    let row = sqlx::query(
        r#"INSERT INTO user_food_mark (user_id, food_id, mark_type)
           VALUES ($1, $2, $3::mark_type_enum)
           RETURNING created_at"#,
    )
    .bind(token.user_id)
    .bind(food_id)
    .bind(input.mark_type.as_str())
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| CustomError::invalid_parameter("mark_type 必须是 LIKE 或 NOT_RECOMMEND"))?;

    Ok(ApiResponse::success(FoodMarkOut {
        food_id,
        user_id: token.user_id,
        mark_type: input.mark_type.as_str().to_string(),
        created_at: row.get("created_at"),
    }))
}

/// 取消标记
#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/foods/{food_id}/mark",
    tag = "菜品标记 (§24.6)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("food_id" = i64, Path, description = "菜品 ID")
    ),
    security(("bearer_auth" = []))
)]
pub async fn unmark_food(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (_group_id, food_id) = path.into_inner();

    let rows = sqlx::query("DELETE FROM user_food_mark WHERE user_id = $1 AND food_id = $2")
        .bind(token.user_id)
        .bind(food_id)
        .execute(&state.db_pool)
        .await?
        .rows_affected();

    Ok(ApiResponse::success(serde_json::json!({ "deleted": rows })))
}

/// 获取菜品标记
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/foods/{food_id}/mark",
    tag = "菜品标记 (§24.6)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("food_id" = i64, Path, description = "菜品 ID")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_food_mark(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (_group_id, food_id) = path.into_inner();

    let row = sqlx::query(
        "SELECT mark_type::text, created_at FROM user_food_mark WHERE user_id = $1 AND food_id = $2"
    )
    .bind(token.user_id)
    .bind(food_id)
    .fetch_optional(&state.db_pool)
    .await?;

    match row {
        Some(r) => Ok(ApiResponse::success(Some(FoodMarkOut {
            food_id,
            user_id: token.user_id,
            mark_type: r.get("mark_type"),
            created_at: r.get("created_at"),
        }))),
        None => Ok(ApiResponse::success(Option::<FoodMarkOut>::None)),
    }
}
