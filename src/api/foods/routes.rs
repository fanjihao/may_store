// API 层 - 菜品路由
// 处理菜品、标签、食材相关的 HTTP 请求

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    domain::foods::food::{
        BlindBoxDrawInput, BlindBoxDrawResultOut, FoodCreateInput, FoodFilterQuery,
        FoodMarkActionInput, FoodOut, FoodUpdateInput, MarkTypeEnum,
    },
    domain::foods::ingredient::{
        BatchIngredientSortInput, IngredientCreateInput, IngredientOut, IngredientQuery,
        IngredientUpdateInput,
    },
    application::food_service::{FoodService, IngredientService},
    models::pagination::CursorPage,
    middlewares::auth::UserToken,
};

/// 配置菜品路由
pub fn configure(cfg: &mut ServiceConfig) {
    // 菜品路由
    cfg.service(
        web::scope("/foods")
            .route("", web::post().to(create_food))
            .route("", web::get().to(get_foods))
            .route("/{id}", web::get().to(get_food_detail))
            .route("/{id}", web::put().to(update_food))
            .route("/{id}", web::delete().to(delete_food))
            .route("/mark", web::post().to(mark_food))
            .route("/mark/{food_id}/{mark_type}", web::delete().to(unmark_food))
            .route("/marks", web::get().to(get_marked_foods))
            .route("/blind_box/draw", web::post().to(draw_blind_box)),
    )
    // 标签路由
    .service(
        web::scope("/food_tags")
            .route(
                "",
                web::post().to(crate::api::foods::tag_routes::create_tag),
            )
            .route("", web::get().to(crate::api::foods::tag_routes::get_tags))
            .route(
                "/{id}",
                web::put().to(crate::api::foods::tag_routes::update_tag),
            )
            .route(
                "/{id}",
                web::delete().to(crate::api::foods::tag_routes::delete_tag),
            ),
    )
    // 食材路由
    .service(
        web::scope("/ingredients")
            .route("", web::get().to(list_ingredients))
            .route("", web::post().to(create_ingredient))
            .route("/{id}", web::get().to(get_ingredient))
            .route("/{id}", web::put().to(update_ingredient))
            .route("/{id}", web::delete().to(delete_ingredient))
            .route("/sort", web::post().to(update_ingredients_sort)),
    );
}

// ========== 菜品 Handler 函数 ==========

#[utoipa::path(
    post,
    path = "/foods",
    tag = "菜品",
    request_body = FoodCreateInput,
    responses((status = 201, body = FoodOut)),
    security(("cookie_auth" = []))
)]
pub async fn create_food(
    token: UserToken,
    data: Json<FoodCreateInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let out = FoodService::create_food(&state.db_pool, &token, &data).await?;
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    get,
    path = "/foods",
    tag = "菜品",
    params(FoodFilterQuery),
    responses((status = 200, body = CursorPage<FoodOut>)),
    security(("cookie_auth"=[]))
)]
pub async fn get_foods(
    state: State<Arc<AppState>>,
    token: UserToken,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let out = FoodService::get_foods(&state.db_pool, &token, &q).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    get,
    path = "/foods/{id}",
    tag = "菜品",
    params(("id"=i64, Path)),
    responses((status = 200, body = FoodOut))
)]
pub async fn get_food_detail(
    state: State<Arc<AppState>>,
    token: Option<UserToken>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = FoodService::get_food_detail(&state.db_pool, token.as_ref(), *id).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    put,
    path = "/foods/{id}",
    tag = "菜品",
    request_body = FoodUpdateInput,
    params(("id" = i64, Path, description = "菜品ID")),
    responses((status = 200, body = FoodOut)),
    security(("cookie_auth" = []))
)]
pub async fn update_food(
    token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<FoodUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let out = FoodService::update_food(&state.db_pool, &token, *id, &data).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    delete,
    path = "/foods/{id}",
    tag = "菜品",
    params(("id" = i64, Path, description = "菜品ID")),
    responses((status = 204), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn delete_food(
    token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    FoodService::delete_food(&state.db_pool, &token, *id).await?;
    Ok(HttpResponse::NoContent())
}

#[utoipa::path(
    post,
    path = "/foods/mark",
    tag = "菜品",
    request_body = FoodMarkActionInput,
    responses((status = 200, body = String)),
    security(("cookie_auth" = []))
)]
pub async fn mark_food(
    token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<FoodMarkActionInput>,
) -> Result<impl Responder, CustomError> {
    FoodService::mark_food(
        &state.db_pool,
        token.user_id as i64,
        data.food_id,
        data.mark_type.clone(),
    )
    .await?;
    Ok(HttpResponse::Ok().body("ok"))
}

#[utoipa::path(
    delete,
    path = "/foods/mark/{food_id}/{mark_type}",
    tag = "菜品",
    params(("food_id"=i64, Path), ("mark_type"=MarkTypeEnum, Path)),
    responses((status = 200, body = String)),
    security(("cookie_auth" = []))
)]
pub async fn unmark_food(
    token: UserToken,
    state: State<Arc<AppState>>,
    path: Path<(i64, MarkTypeEnum)>,
) -> Result<impl Responder, CustomError> {
    let (food_id, mark_type) = path.into_inner();
    FoodService::unmark_food(&state.db_pool, token.user_id as i64, food_id, mark_type).await?;
    Ok(HttpResponse::Ok().body("ok"))
}

#[utoipa::path(
    get,
    path = "/foods/marks",
    tag = "菜品",
    params(FoodFilterQuery),
    responses((status = 200, body = CursorPage<FoodOut>)),
    security(("cookie_auth" = []))
)]
pub async fn get_marked_foods(
    token: UserToken,
    state: State<Arc<AppState>>,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let out = FoodService::get_marked_foods(&state.db_pool, &token, &q).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    post,
    path = "/foods/blind_box/draw",
    tag = "菜品",
    request_body = BlindBoxDrawInput,
    responses((status = 200, body = BlindBoxDrawResultOut)),
    security(("cookie_auth" = []))
)]
pub async fn draw_blind_box(
    token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<BlindBoxDrawInput>,
) -> Result<impl Responder, CustomError> {
    let out = FoodService::draw_blind_box(&state.db_pool, &token, &data).await?;
    Ok(HttpResponse::Ok().json(&out))
}

// ========== 食材 Handler 函数 ==========

#[utoipa::path(
    get,
    path = "/ingredients",
    tag = "食材",
    params(IngredientQuery),
    responses((status = 200, body = CursorPage<IngredientOut>)),
    security(("cookie_auth" = []))
)]
pub async fn list_ingredients(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<IngredientQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = query
        .group_id
        .or(user_token.user.as_ref().and_then(|u| u.group_id));
    let keyword = query.keyword.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let out = IngredientService::list_ingredients(
        &state.db_pool,
        group_id,
        keyword,
        limit,
        query.cursor.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    get,
    path = "/ingredients/{id}",
    tag = "食材",
    params(("id" = i64, Path, description = "食材ID")),
    responses((status = 200, body = IngredientOut), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn get_ingredient(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = IngredientService::get_ingredient(&state.db_pool, *id).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    post,
    path = "/ingredients",
    tag = "食材",
    request_body = IngredientCreateInput,
    responses((status = 201, body = IngredientOut)),
    security(("cookie_auth" = []))
)]
pub async fn create_ingredient(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<IngredientCreateInput>,
) -> Result<impl Responder, CustomError> {
    let out = IngredientService::create_ingredient(
        &state.db_pool,
        &data,
        user_token.user.as_ref().and_then(|u| u.group_id),
    )
    .await?;
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    put,
    path = "/ingredients/{id}",
    tag = "食材",
    params(("id" = i64, Path, description = "食材ID")),
    request_body = IngredientUpdateInput,
    responses((status = 200, body = IngredientOut), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn update_ingredient(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<IngredientUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let out = IngredientService::update_ingredient(&state.db_pool, *id, &data).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    delete,
    path = "/ingredients/{id}",
    tag = "食材",
    params(("id" = i64, Path, description = "食材ID")),
    responses((status = 204), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn delete_ingredient(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    IngredientService::delete_ingredient(&state.db_pool, *id).await?;
    Ok(HttpResponse::NoContent())
}

#[utoipa::path(
    post,
    path = "/ingredients/sort",
    tag = "食材",
    request_body = BatchIngredientSortInput,
    responses((status = 200, body = String)),
    security(("cookie_auth" = []))
)]
pub async fn update_ingredients_sort(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<BatchIngredientSortInput>,
) -> Result<impl Responder, CustomError> {
    IngredientService::update_ingredients_sort(&state.db_pool, &data).await?;
    Ok(HttpResponse::Ok().body("ok"))
}
