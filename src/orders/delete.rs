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
use sqlx::Row;

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
    let mut order = crate::models::orders::OrderRecord {
        order_id: row.get("order_id"),
        user_id: row.get("user_id"),
        receiver_id: row.get("receiver_id"),
        group_id: row.get("group_id"),
        status: row.get::<OrderStatusEnum, _>("status"),
        goal_time: row.try_get("goal_time").ok(),
        points_cost: row.get("points_cost"),
        points_reward: row.get("points_reward"),
        cancel_reason: row.try_get("cancel_reason").ok(),
        reject_reason: row.try_get("reject_reason").ok(),
        last_status_change_at: row.try_get("last_status_change_at").ok(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
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
    let item_rows = sqlx::query("SELECT oi.id, oi.order_id, oi.food_id, oi.quantity, oi.price, oi.snapshot_json, oi.created_at, f.food_name, f.food_photo \
        FROM order_items oi LEFT JOIN foods f ON f.food_id = oi.food_id WHERE oi.order_id=$1")
		.bind(order.order_id)
		.fetch_all(&mut *tx)
		.await?;
    let items: Vec<OrderItemOut> = item_rows
        .into_iter()
        .map(super::view::map_item_record_to_out(&state.db_pool))
        .collect::<Result<Vec<_>, _>>()?;
    let hist_rows = sqlx::query("SELECT h.from_status::text, h.to_status::text, u.nick_name, h.remark, h.changed_at \
        FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
        WHERE h.order_id=$1 ORDER BY h.changed_at")
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
