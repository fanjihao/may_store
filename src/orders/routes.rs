use crate::models::pagination::CursorPage;
use crate::orders::service::OrderService;
use crate::{
    config::AppState,
    errors::CustomError,
    models::users::UserToken,
    orders::models::{
        OrderCreateInput, OrderOutNew, OrderQuery, OrderRatingCreateInput, OrderRatingOut,
        OrderStatusUpdateInput,
    },
};
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

#[utoipa::path(
	post,
	path = "/orders",
	tag = "订单",
	request_body = OrderCreateInput,
	responses((status = 201, body = OrderOutNew))
)]
pub async fn create_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<OrderCreateInput>,
) -> Result<impl Responder, CustomError> {
    let out = OrderService::create_order(&state.db_pool, &user_token, &data.into_inner()).await?;
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    get,
    path = "/orders",
    tag = "订单",
    params(OrderQuery),
    responses((status = 200, body = CursorPage<OrderOutNew>))
)]
pub async fn get_orders(
    token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<OrderQuery>,
) -> Result<impl Responder, CustomError> {
    let page = OrderService::get_orders(&state.db_pool, &token, &query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(&page))
}

#[utoipa::path(
    get,
    path = "/orders/{id}",
    tag = "订单",
    params(("id" = i64, Path, description = "订单ID")),
    responses((status = 200, body = OrderOutNew))
)]
pub async fn get_order_detail(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = OrderService::get_order_detail(&state.db_pool, Some(&_user_token), *id).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    get,
    path = "/orders-incomplete/{user_id}",
    tag = "订单",
    params(("user_id" = i64, Path, description = "用户ID")),
    responses((status = 200, body = i32))
)]
pub async fn get_incomplete_order(
    state: State<Arc<AppState>>,
    user_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let count = OrderService::get_incomplete_order(&state.db_pool, *user_id).await?;
    Ok(HttpResponse::Ok().json(&count))
}

#[utoipa::path(
    put,
    path = "/orders/status",
    tag = "订单",
    request_body = OrderStatusUpdateInput,
    responses((status = 200, body = OrderOutNew))
)]
pub async fn update_order_status(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<OrderStatusUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let out =
        OrderService::update_order_status(&state.db_pool, &user_token, &data.into_inner()).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
	delete,
	path = "/orders/{id}",
	tag = "订单",
	params(("id" = i64, Path, description = "订单ID")),
	responses((status = 200, description = "订单删除成功"))
)]
pub async fn delete_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    OrderService::delete_order(&state.db_pool, &user_token, *id).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "description": "订单删除成功"
    })))
}

#[utoipa::path(
    post,
    path = "/orders-rating/{order_id}",
    tag = "评分",
    params(("order_id" = i64, Path, description = "订单ID")),
    request_body = OrderRatingCreateInput,
    responses((status = 201, body = OrderRatingOut))
)]
pub async fn create_order_rating(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
    body: Json<OrderRatingCreateInput>,
) -> Result<impl Responder, CustomError> {
    let out = OrderService::create_order_rating(
        &state.db_pool,
        &user_token,
        *order_id,
        &body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    get,
    path = "/orders-rating/{order_id}",
    tag = "评分",
    params(("order_id" = i64, Path, description = "订单ID")),
    responses((status = 200, body = OrderRatingOut))
)]
pub async fn get_order_rating(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = OrderService::get_order_rating(&state.db_pool, &user_token, *order_id).await?;
    Ok(HttpResponse::Ok().json(&out))
}
