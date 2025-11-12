use crate::models::users::UserToken;
use crate::{
    errors::CustomError,
    models::orders::{OrderItemOut, OrderOutNew, OrderStatusEnum, OrderStatusHistoryOut},
    AppState,
};
use ntex::web::{
    types::{Json, Path, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

#[derive(Debug, serde::Deserialize)]
pub struct CancelInput {
    pub reason: Option<String>,
}

#[utoipa::path(
	delete,
	path = "/orders/{id}",
	tag = "订单",
	params(("id" = i64, Path, description = "订单ID")),
	responses((status = 200, body = OrderOutNew))
)]
pub async fn delete_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<CancelInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;
    let row = sqlx::query("SELECT order_id, user_id, receiver_id, group_id, status, goal_time, points_cost, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at FROM orders WHERE order_id=$1 FOR UPDATE")
		.bind(*id)
		.fetch_optional(&mut *tx)
		.await?;
    let row = match row {
        Some(r) => r,
        None => {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("订单不存在".into()));
        }
    };
    let mut order = super::view::map_record(&row);
    if order.status != OrderStatusEnum::PENDING {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("仅待处理订单可取消".into()));
    }
    if order.user_id != user_token.user_id {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("只能取消自己创建的订单".into()));
    }

    // 取消
    sqlx::query("UPDATE orders SET status='CANCELLED', cancel_reason=$2, last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1")
		.bind(order.order_id)
		.bind(&body.reason)
		.execute(&mut *tx)
		.await?;
    order.status = OrderStatusEnum::CANCELLED;
    order.cancel_reason = body.reason.clone();
    order.last_status_change_at = Some(chrono::Utc::now());

    // 历史
    sqlx::query("INSERT INTO order_status_history (order_id, from_status, to_status, changed_by, remark) VALUES ($1,$2,$3,$4,$5)")
		.bind(order.order_id)
		.bind("PENDING")
		.bind("CANCELLED")
		.bind(user_token.user_id as i64)
		.bind(body.reason.clone())
		.execute(&mut *tx)
		.await?;

    // items
    let item_rows = sqlx::query("SELECT id, order_id, food_id, quantity, price, snapshot_json, created_at FROM order_items WHERE order_id=$1")
		.bind(order.order_id)
		.fetch_all(&mut *tx)
		.await?;
    let items: Vec<OrderItemOut> = item_rows
        .into_iter()
        .map(super::view::map_item_record_to_out(&state.db_pool))
        .collect::<Result<Vec<_>, _>>()?;
    let hist_rows = sqlx::query("SELECT from_status::text, to_status::text, changed_by, remark, changed_at FROM order_status_history WHERE order_id=$1 ORDER BY changed_at")
		.bind(order.order_id)
		.fetch_all(&mut *tx)
		.await?;
    let history: Vec<OrderStatusHistoryOut> = hist_rows
        .into_iter()
        .map(super::view::map_history_row)
        .collect();
    tx.commit().await?;
    // 异步推送取消状态
    {
        let pool_clone = state.db_pool.clone();
        let oid = order.order_id;
        tokio::spawn(async move {
            if let Err(e) = crate::services::notifications::push_order_status(oid, pool_clone).await {
                log::warn!("order cancel push error: {}", e);
            }
        });
    }
    Ok(HttpResponse::Ok().json(&OrderOutNew::from((order, items, history))))
}
