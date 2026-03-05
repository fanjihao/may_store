use crate::{
    config::AppState,
    dashboard::models::{
        DateFoodsResponse, DateQuery, GroupActivityEventOut, GroupActivityQuery, OrderStatsOut,
        PointsJourneyOut, TodayOrdersResponse, TopFoodRankingResponse, WeekOrderDatesOut,
    },
    errors::CustomError,
    middlewares::auth::UserToken,
    models::pagination::CursorPage,
};
use ntex::web::{
    types::{Path, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

use super::service::DashboardService;

#[utoipa::path(
    get,
    path="/groups/{group_id}/activities",
    tag="看板",
    params(
        ("group_id"=i64, Path, description="组ID"),
        GroupActivityQuery
    ),
    responses((
        status=200,
        body=CursorPage<GroupActivityEventOut>
    )),
    security(("cookie_auth"=[]))
)]
pub async fn get_group_activities(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<GroupActivityQuery>,
) -> Result<impl Responder, CustomError> {
    let result = DashboardService::get_group_activities(&state.db_pool, *group_id, &query).await?;
    Ok(HttpResponse::Ok().json(&result))
}

#[utoipa::path(
    get,
    path="/dashboard/top-foods",
    tag="看板",
    responses((status=200, body=TopFoodRankingResponse)),
    security(("cookie_auth"=[]))
)]
pub async fn get_top_food_orders(
    state: State<Arc<AppState>>,
    _user: UserToken,
) -> Result<impl Responder, CustomError> {
    let result = DashboardService::get_top_food_orders(&state.db_pool).await?;
    Ok(HttpResponse::Ok().json(&result))
}

#[utoipa::path(
    get,
    path="/dashboard/my/orders-today",
    tag="看板",
    responses((status=200, body=TodayOrdersResponse)),
    security(("cookie_auth"=[]))
)]
pub async fn get_my_today_orders(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let result = DashboardService::get_my_today_orders(&state.db_pool, user.user_id).await?;
    Ok(HttpResponse::Ok().json(&result))
}

#[utoipa::path(
    get,
    path="/dashboard/my/order-stats",
    tag="看板",
    responses((status=200, body=OrderStatsOut)),
    security(("cookie_auth"=[]))
)]
pub async fn get_my_order_stats(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let result = DashboardService::get_my_order_stats(&state.db_pool, user.user_id).await?;
    Ok(HttpResponse::Ok().json(&result))
}

#[utoipa::path(
    get,
    path="/dashboard/my/points-journey",
    tag="看板",
    responses((status=200, body=PointsJourneyOut)),
    security(("cookie_auth"=[]))
)]
pub async fn get_points_journey(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let result = DashboardService::get_points_journey(&state.db_pool, user.user_id).await?;
    Ok(HttpResponse::Ok().json(&result))
}

#[utoipa::path(
    get,
    path = "/dashboard/week-order-dates",
    tag = "看板",
    params(DateQuery),
    responses((status = 200, body = WeekOrderDatesOut)),
    security(("cookie_auth" = []))
)]
pub async fn get_week_order_dates(
    state: State<Arc<AppState>>,
    user: UserToken,
    query: Query<DateQuery>,
) -> Result<impl Responder, CustomError> {
    let result =
        DashboardService::get_week_order_dates(&state.db_pool, user.user_id, &query).await?;
    Ok(HttpResponse::Ok().json(&result))
}

#[utoipa::path(
    get,
    path = "/dashboard/date-foods",
    tag = "看板",
    params(DateQuery),
    responses((status = 200, body = DateFoodsResponse)),
    security(("cookie_auth" = []))
)]
pub async fn get_date_foods(
    state: State<Arc<AppState>>,
    user: UserToken,
    query: Query<DateQuery>,
) -> Result<impl Responder, CustomError> {
    let result = DashboardService::get_date_foods(&state.db_pool, user.user_id, &query).await?;
    Ok(HttpResponse::Ok().json(&result))
}
