use chrono::Utc;
use ntex::web::{types::{Json, State}, HttpResponse, Responder};
use sqlx::Row;
use std::sync::Arc;
use crate::{
    errors::CustomError,
    models::{
        orders::{OrderStatusUpdateInput, OrderStatusEnum, OrderRecord, OrderItemRecord, OrderItemOut, OrderStatusHistoryOut, OrderOutNew},
        users::UserToken,
    },
    AppState
};

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
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // 当前订单
    let current: Option<OrderRecord> = sqlx::query_as::<_, OrderRecord>(
        "SELECT order_id, user_id, receiver_id, group_id, status, goal_time, points_cost, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at FROM orders WHERE order_id=$1 FOR UPDATE"
    )
    .bind(data.order_id)
    .fetch_optional(&mut *tx)
    .await?;
    let mut order = match current { Some(o) => o, None => { tx.rollback().await.ok(); return Err(CustomError::BadRequest("订单不存在".into())); } };
    let from_status = order.status;

    if order.status == data.to_status { tx.rollback().await.ok(); return Err(CustomError::BadRequest("状态未变化".into())); }
    if !order.status.can_transition(data.to_status) { tx.rollback().await.ok(); return Err(CustomError::BadRequest("非法状态流转".into())); }

    // 更新 order 主表
    match data.to_status {
        OrderStatusEnum::REJECTED => {
            sqlx::query(
                "UPDATE orders SET status=$2, reject_reason=$3, last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1"
            )
            .bind(order.order_id)
            .bind(data.to_status)
            .bind(&data.remark)
            .execute(&mut *tx)
            .await?;
            order.reject_reason = data.remark.clone();
        }
        OrderStatusEnum::CANCELLED => {
            sqlx::query(
                "UPDATE orders SET status=$2, cancel_reason=$3, last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1"
            )
            .bind(order.order_id)
            .bind(data.to_status)
            .bind(&data.remark)
            .execute(&mut *tx)
            .await?;
            order.cancel_reason = data.remark.clone();
        }
        _ => {
            sqlx::query(
                "UPDATE orders SET status=$2, last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1"
            )
            .bind(order.order_id)
            .bind(data.to_status)
            .execute(&mut *tx)
            .await?;
        }
    }
    order.status = data.to_status;
    order.last_status_change_at = Some(Utc::now());

    // 积分奖励处理（完成时）
    if data.to_status == OrderStatusEnum::FINISHED {
        if let Some(points) = data.points_reward.or(Some(order.points_reward)).filter(|p| *p > 0) {
            // 获取当前积分并更新
            if let Ok(user_row) = sqlx::query("SELECT love_point FROM users WHERE user_id=$1")
                .bind(order.user_id)
                .fetch_one(&mut *tx)
                .await {
                let current_lp: i32 = user_row.get("love_point");
                let balance_after = current_lp + points;
                sqlx::query("INSERT INTO point_transactions (user_id, amount, type, ref_type, ref_id, balance_after) VALUES ($1,$2,'FINISH_REWARD',1,$3,$4)")
                    .bind(order.user_id)
                    .bind(points)
                    .bind(order.order_id)
                    .bind(balance_after)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
                    .bind(order.user_id)
                    .bind(balance_after)
                    .execute(&mut *tx)
                    .await?;
                order.points_reward = points; // reflect final awarded points
            }
        }
    }

    // 记录历史
    sqlx::query("INSERT INTO order_status_history (order_id, from_status, to_status, changed_by, remark) VALUES ($1,$2,$3,$4,$5)")
        .bind(order.order_id)
        .bind(from_status)
        .bind(data.to_status)
        .bind(user_token.user_id as i64)
        .bind(&data.remark)
        .execute(&mut *tx)
        .await?;

    // 查询 items
    let item_rows: Vec<OrderItemRecord> = sqlx::query_as::<_, OrderItemRecord>(
        "SELECT id, order_id, food_id, quantity, price, snapshot_json, created_at FROM order_items WHERE order_id=$1"
    )
    .bind(order.order_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut items_out: Vec<OrderItemOut> = Vec::new();
    for ir in item_rows {
        let food = sqlx::query("SELECT food_name, food_photo FROM foods WHERE food_id=$1")
            .bind(ir.food_id)
            .fetch_optional(&mut *tx)
            .await?;
        let (name_opt, photo_opt) = food
            .map(|r| (r.get::<String, _>("food_name"), r.get::<Option<String>, _>("food_photo")))
            .map(|(n, p)| (Some(n), p))
            .unwrap_or((None, None));
        items_out.push(OrderItemOut {
            id: ir.id,
            food_id: ir.food_id,
            food_name: name_opt,
            food_photo: photo_opt,
            quantity: ir.quantity,
            price: ir.price,
        });
    }

    // 历史列表
    let history_rows: Vec<OrderStatusHistoryOut> = sqlx::query(
        "SELECT h.from_status::text, h.to_status::text, u.nick_name, h.remark, h.changed_at \
         FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
         WHERE h.order_id=$1 ORDER BY h.changed_at"
    )
    .bind(order.order_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| {
        let from_s = row.get::<Option<String>, _>("from_status")
            .and_then(|s| match s.as_str() {
                "PENDING" => Some(OrderStatusEnum::PENDING),
                "ACCEPTED" => Some(OrderStatusEnum::ACCEPTED),
                "FINISHED" => Some(OrderStatusEnum::FINISHED),
                "CANCELLED" => Some(OrderStatusEnum::CANCELLED),
                "EXPIRED" => Some(OrderStatusEnum::EXPIRED),
                "REJECTED" => Some(OrderStatusEnum::REJECTED),
                "SYSTEM_CLOSED" => Some(OrderStatusEnum::SYSTEM_CLOSED),
                _ => None,
            });
        let to_s_str: String = row.get("to_status");
        let to_s = match to_s_str.as_str() {
            "PENDING" => OrderStatusEnum::PENDING,
            "ACCEPTED" => OrderStatusEnum::ACCEPTED,
            "FINISHED" => OrderStatusEnum::FINISHED,
            "CANCELLED" => OrderStatusEnum::CANCELLED,
            "EXPIRED" => OrderStatusEnum::EXPIRED,
            "REJECTED" => OrderStatusEnum::REJECTED,
            "SYSTEM_CLOSED" => OrderStatusEnum::SYSTEM_CLOSED,
            _ => OrderStatusEnum::PENDING,
        };
        OrderStatusHistoryOut {
            from_status: from_s,
            to_status: to_s,
            changed_by: row.try_get("nick_name").ok().flatten(),
            remark: row.get::<Option<String>, _>("remark"),
            changed_at: row.get::<chrono::DateTime<Utc>, _>("changed_at")
        }
    })
    .collect();

    tx.commit().await?;

    // 异步推送状态更新
    {
        let pool_clone = state.db_pool.clone();
        let oid = order.order_id;
        tokio::spawn(async move {
            if let Err(e) = crate::services::notifications::push_order_status(oid, pool_clone).await {
                log::warn!("order status update push error: {}", e);
            }
        });
    }

    let out = OrderOutNew::from((order, items_out, history_rows));
    Ok(HttpResponse::Ok().json(&out))
}
