// API 层 - 订单路由
// FSD.latest.md compliant - 仅保留 FSD 核心 API

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::application::order_service::OrderService as AppOrderService;
use crate::config::AppState;
use crate::domain::order::{
    OrderCreateInput, OrderOutNew, OrderQuery, OrderRatingCreateInput, OrderRatingOut, OrderStatus,
    OrderStatusUpdateInput,
};
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::models::pagination::CursorPage;
use crate::utils::response::ApiResponse;

/// 配置订单路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/orders")
            .route("", web::post().to(create_order))
            .route("", web::get().to(get_orders))
            .route("/{order_id}", web::get().to(get_order_detail))
            // FSD v2: 独立接单/完成/确认接口
            .route("/{order_id}/accept", web::post().to(accept_order))
            .route("/{order_id}/complete", web::post().to(complete_order))
            .route("/{order_id}/confirm", web::post().to(confirm_order))
            // FSD v2: 取消/拒绝/超时/备注接口
            .route("/{order_id}/cancel", web::post().to(cancel_order))
            .route("/{order_id}/reject", web::post().to(reject_order))
            .route("/{order_id}/timeout", web::post().to(order_timeout))
            .route("/{order_id}/guest-remark", web::patch().to(update_guest_remark)),
    )
    .service(
        web::scope("/api/orders-rating")
            .route("/{order_id}", web::post().to(create_order_rating))
            .route("/{order_id}", web::get().to(get_order_rating)),
    );
}

#[utoipa::path(
    post,
    path = "/api/orders",
    tag = "订单",
    request_body = OrderCreateInput,
    responses((status = 201, body = OrderOutNew)),
    security(("bearer_auth" = []))
)]
pub async fn create_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<OrderCreateInput>,
) -> Result<impl Responder, CustomError> {
    let out = AppOrderService::create_order(&state.db_pool, user_token.user_id, &data.into_inner())
        .await?;
    Ok(ApiResponse::success(out))
}

#[utoipa::path(
    get,
    path = "/api/orders",
    tag = "订单",
    params(OrderQuery),
    responses((status = 200, body = CursorPage<OrderOutNew>)),
    security(("bearer_auth" = []))
)]
pub async fn get_orders(
    token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<OrderQuery>,
) -> Result<impl Responder, CustomError> {
    let page = AppOrderService::get_orders(&state.db_pool, &token, &query.into_inner()).await?;
    Ok(ApiResponse::success(page))
}

#[utoipa::path(
    get,
    path = "/api/orders/{order_id}",
    tag = "订单",
    params(("order_id" = i64, Path)),
    responses((status = 200, body = OrderOutNew)),
    security(("bearer_auth" = []))
)]
pub async fn get_order_detail(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = AppOrderService::get_order_by_id(&state.db_pool, *id).await?;
    Ok(ApiResponse::success(out))
}

#[utoipa::path(
    post,
    path = "/api/orders-rating/{order_id}",
    tag = "评分",
    params(("order_id" = i64, Path)),
    request_body = OrderRatingCreateInput,
    responses((status = 201, body = OrderRatingOut)),
    security(("bearer_auth" = []))
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
    )
    .await?;
    Ok(ApiResponse::success(out))
}

#[utoipa::path(
    get,
    path = "/api/orders-rating/{order_id}",
    tag = "评分",
    params(("order_id" = i64, Path)),
    responses((status = 200, body = OrderRatingOut)),
    security(("bearer_auth" = []))
)]
pub async fn get_order_rating(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out =
        AppOrderService::get_order_rating(&state.db_pool, user_token.user_id, *order_id).await?;
    Ok(ApiResponse::success(out))
}

// ============== FSD v2 独立接口: 接单/完成/确认 ==============

/// 接单 - Seller 接受订单
/// POST /api/orders/{order_id}/accept
#[utoipa::path(
    post,
    path = "/api/orders/{order_id}/accept",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    responses(
        (status = 200, description = "接单成功"),
        (status = 400, description = "订单状态不允许接单"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn accept_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let id = *order_id;
    let input = OrderStatusUpdateInput {
        order_id: id,
        to_status: OrderStatus::Accepted,
        remark: None,
        points_reward: None,
    };
    let out =
        AppOrderService::update_order_status(&state.db_pool, user_token.user_id, &input).await?;
    Ok(ApiResponse::success(out))
}

/// 完成订单 - Seller 完成任务制作/履约
/// POST /api/orders/{order_id}/complete
#[utoipa::path(
    post,
    path = "/api/orders/{order_id}/complete",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    responses(
        (status = 200, description = "完成成功"),
        (status = 400, description = "订单状态不允许完成"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn complete_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let id = *order_id;
    let input = OrderStatusUpdateInput {
        order_id: id,
        to_status: OrderStatus::ProductionCompleted,
        remark: None,
        points_reward: None,
    };
    let out =
        AppOrderService::update_order_status(&state.db_pool, user_token.user_id, &input).await?;
    Ok(ApiResponse::success(out))
}

/// 确认订单 - Buyer 确认履约质量
/// POST /api/orders/{order_id}/confirm
#[utoipa::path(
    post,
    path = "/api/orders/{order_id}/confirm",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    request_body = OrderConfirmInput,
    responses(
        (status = 200, description = "确认成功"),
        (status = 400, description = "订单状态不允许确认"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn confirm_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
    body: Json<OrderConfirmInput>,
) -> Result<impl Responder, CustomError> {
    let id = *order_id;
    let input = body.into_inner();
    let to_status = if input.is_complete {
        OrderStatus::ConfirmedCompleted
    } else {
        OrderStatus::ConfirmedIncomplete
    };
    let out = AppOrderService::update_order_status(
        &state.db_pool,
        user_token.user_id,
        &OrderStatusUpdateInput {
            order_id: id,
            to_status,
            remark: input.remark,
            points_reward: None,
        },
    )
    .await?;
    Ok(ApiResponse::success(out))
}

/// 取消订单
/// POST /api/orders/{order_id}/cancel
///
/// 仅 CREATED 状态可取消
#[utoipa::path(
    post,
    path = "/api/orders/{order_id}/cancel",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    request_body = OrderCancelInput,
    responses(
        (status = 200, description = "取消成功"),
        (status = 400, description = "订单状态不允许取消"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn cancel_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
    body: Json<OrderCancelInput>,
) -> Result<impl Responder, CustomError> {
    let id = *order_id;
    let input = body.into_inner();
    let out = AppOrderService::update_order_status(
        &state.db_pool,
        user_token.user_id,
        &OrderStatusUpdateInput {
            order_id: id,
            to_status: OrderStatus::Cancelled,
            remark: input.reason,
            points_reward: None,
        },
    )
    .await?;
    Ok(ApiResponse::success(out))
}

/// 拒绝订单
/// POST /api/orders/{order_id}/reject
///
/// 仅 CREATED 状态 Seller 可拒绝
#[utoipa::path(
    post,
    path = "/api/orders/{order_id}/reject",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    request_body = OrderRejectInput,
    responses(
        (status = 200, description = "拒绝成功"),
        (status = 400, description = "订单状态不允许拒绝"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn reject_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
    body: Json<OrderRejectInput>,
) -> Result<impl Responder, CustomError> {
    let id = *order_id;
    let input = body.into_inner();
    let out = AppOrderService::update_order_status(
        &state.db_pool,
        user_token.user_id,
        &OrderStatusUpdateInput {
            order_id: id,
            to_status: OrderStatus::Rejected,
            remark: input.reason,
            points_reward: None,
        },
    )
    .await?;
    Ok(ApiResponse::success(out))
}

/// 订单超时处理
/// POST /api/orders/{order_id}/timeout
///
/// CREATED 或 ACCEPTED 状态超过超时时间可标记为超时
#[utoipa::path(
    post,
    path = "/api/orders/{order_id}/timeout",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    responses(
        (status = 200, description = "处理成功"),
        (status = 400, description = "订单状态不允许超时处理"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn order_timeout(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let id = *order_id;
    let out = AppOrderService::update_order_status(
        &state.db_pool,
        user_token.user_id,
        &OrderStatusUpdateInput {
            order_id: id,
            to_status: OrderStatus::Timeout,
            remark: None,
            points_reward: None,
        },
    )
    .await?;
    Ok(ApiResponse::success(out))
}

/// 更新做客订单备注
/// PATCH /api/orders/{order_id}/guest-remark
///
/// 仅做客订单创建者可更新备注
#[utoipa::path(
    patch,
    path = "/api/orders/{order_id}/guest-remark",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    request_body = GuestRemarkInput,
    responses(
        (status = 200, description = "更新成功"),
        (status = 400, description = "非做客订单无法更新备注"),
        (status = 403, description = "无权更新"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_guest_remark(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
    body: Json<GuestRemarkInput>,
) -> Result<impl Responder, CustomError> {
    let id = *order_id;
    let input = body.into_inner();

    // 检查订单是否存在且为 GUEST 类型
    let order = AppOrderService::get_order_by_id(&state.db_pool, id).await?
        .ok_or_else(|| CustomError::NotFound("订单不存在".into()))?;

    if !order.is_guest {
        return Err(CustomError::BadRequest("非做客订单无法更新备注".into()));
    }

    // 检查是否是创建者
    if order.user_id != user_token.user_id {
        return Err(CustomError::Forbidden("无权更新此订单备注".into()));
    }

    // 更新做客备注
    sqlx::query(
        "UPDATE orders SET guest_remark=$1, guest_mark_tags=$2 WHERE order_id=$3"
    )
    .bind(&input.guest_remark)
    .bind(serde_json::json!(&input.guest_mark_tags))
    .bind(id)
    .execute(&state.db_pool)
    .await?;

    Ok(ApiResponse::ok())
}

/// 订单取消输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderCancelInput {
    pub reason: Option<String>,
}

/// 订单拒绝输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderRejectInput {
    pub reason: Option<String>,
}

/// 做客订单备注输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuestRemarkInput {
    pub guest_remark: Option<String>,
    pub guest_mark_tags: Option<Vec<String>>,
}

/// 订单确认输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderConfirmInput {
    pub is_complete: bool,
    pub remark: Option<String>,
}
