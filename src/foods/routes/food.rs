use crate::foods::service::food::FoodService;
use crate::{
    config::AppState,
    errors::CustomError,
    foods::models::food::{
        BlindBoxDrawInput, BlindBoxDrawResultOut, FoodCreateInput, FoodFilterQuery,
        FoodMarkActionInput, FoodOut, FoodUpdateInput, MarkTypeEnum,
    },
    models::pagination::CursorPage,
    users::models::user::UserToken,
};
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

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
