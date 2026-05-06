// API 层 - 订单路由
// 处理订单相关的 HTTP 请求

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use std::sync::Arc;

use crate::application::order_service::OrderService as AppOrderService;
use crate::config::AppState;
use crate::domain::order::{
    OrderCreateInput, OrderOutNew, OrderQuery, OrderRatingCreateInput, OrderRatingOut,
    OrderStatistics, OrderStatusUpdateInput,
};
use crate::middlewares::auth::UserToken;
use crate::errors::CustomError;
use crate::models::pagination::CursorPage;

/// 配置订单路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/orders")
            .route("", web::post().to(create_order))
            .route("", web::get().to(get_orders))
            .route("/team-today", web::get().to(get_team_today_orders))
            .route("/status", web::put().to(update_order_status))
            .route("/{id}", web::get().to(get_order_detail))
            .route("/{id}", web::delete().to(delete_order)),
    )
    .service(
        web::scope("/orders-statistics")
            .route("/{groupId}", web::get().to(get_order_statistics)),
    )
    .service(
        web::scope("/orders-rating")
            .route("/{id}", web::post().to(create_order_rating))
            .route("/{id}", web::get().to(get_order_rating)),
    );
}

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
    let out = AppOrderService::create_order(&state.db_pool, user_token.user_id, &data.into_inner()).await?;
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
    let page = AppOrderService::get_orders(&state.db_pool, &token, &query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(&page))
}

#[utoipa::path(
    get,
    path = "/orders/team-today",
    tag = "订单",
    params(("group_id" = i64, Query)),
    responses((status = 200, body = Vec<OrderOutNew>))
)]
pub async fn get_team_today_orders(
    token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<crate::domain::order::TeamTodayOrdersQuery>,
) -> Result<impl Responder, CustomError> {
    let orders = AppOrderService::get_team_today_orders(
        &state.db_pool,
        token.user_id,
        &query.into_inner(),
    ).await?;
    Ok(HttpResponse::Ok().json(&orders))
}

#[utoipa::path(
    get,
    path = "/orders/{id}",
    tag = "订单",
    params(("id" = i64, Path)),
    responses((status = 200, body = OrderOutNew))
)]
pub async fn get_order_detail(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = AppOrderService::get_order_by_id(&state.db_pool, *id).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    get,
    path = "/orders-statistics/{groupId}",
    tag = "订单",
    params(("groupId" = i64, Path)),
    responses((status = 200, body = OrderStatistics))
)]
pub async fn get_order_statistics(
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let count = AppOrderService::get_order_statistics(&state.db_pool, *group_id).await?;
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
    let out = AppOrderService::update_order_status(
        &state.db_pool,
        user_token.user_id,
        &data.into_inner(),
    ).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    delete,
    path = "/orders/{id}",
    tag = "订单",
    params(("id" = i64, Path)),
    responses((status = 200, description = "订单删除成功"))
)]
pub async fn delete_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    AppOrderService::delete_order(&state.db_pool, user_token.user_id, *id).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({"status": "ok"})))
}

#[utoipa::path(
    post,
    path = "/orders-rating/{id}",
    tag = "评分",
    params(("id" = i64, Path)),
    request_body = OrderRatingCreateInput,
    responses((status = 201, body = OrderRatingOut))
)]
pub async fn create_order_rating(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
    body: Json<OrderRatingCreateInput>,
) -> Result<impl Responder, CustomError> {
    let out = AppOrderService::create_order_rating(
        &state.db_pool,
        user_token.user_id,
        *order_id,
        &body.into_inner(),
    ).await?;
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    get,
    path = "/orders-rating/{id}",
    tag = "评分",
    params(("id" = i64, Path)),
    responses((status = 200, body = OrderRatingOut))
)]
pub async fn get_order_rating(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = AppOrderService::get_order_rating(&state.db_pool, user_token.user_id, *order_id).await?;
    Ok(HttpResponse::Ok().json(&out))
}
