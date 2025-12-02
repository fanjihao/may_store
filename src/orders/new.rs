use std::sync::Arc;

use crate::{
    errors::CustomError,
    models::{
        orders::{
            OrderCreateInput, OrderItemOut, OrderItemRecord, OrderOutNew, OrderRecord,
            OrderStatusEnum, OrderStatusHistoryOut,
        },
        users::UserToken,
    },
    AppState,
};
use chrono::{DateTime, Utc};
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use sqlx::Row;

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
    if data.items.is_empty() {
        return Err(CustomError::BadRequest("缺少菜品".into()));
    }
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // 校验 group_id 成员关系（如提供）
    if let Some(gid) = data.group_id {
        let is_member = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
        )
        .bind(gid)
        .bind(user_token.user_id as i64)
        .fetch_one(&mut *tx)
        .await?;
        if !is_member {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("你不是该组成员".into()));
        }
    }

    let points_cost = data.points_cost.unwrap_or(0);
    let points_reward = data.points_reward.unwrap_or(0);

    // 插入订单并直接解码枚举
    let rec: OrderRecord = sqlx::query_as::<_, OrderRecord>(
        "INSERT INTO orders (user_id, receiver_id, group_id, goal_time, points_cost, points_reward) \
         VALUES ($1,$2,$3,$4,$5,$6) \
         RETURNING order_id, user_id, receiver_id, group_id, status, goal_time, points_cost, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at"
    )
    .bind(user_token.user_id as i64)
    .bind::<Option<i64>>(None)
    .bind(data.group_id)
    .bind(data.goal_time)
    .bind(points_cost)
    .bind(points_reward)
    .fetch_one(&mut *tx)
    .await?;

    // 批量插入条目
    for item in &data.items {
        let qty = item.quantity.unwrap_or(1).max(1);
        sqlx::query("INSERT INTO order_items (order_id, food_id, quantity) VALUES ($1,$2,$3)")
            .bind(rec.order_id)
            .bind(item.food_id)
            .bind(qty)
            .execute(&mut *tx)
            .await?;
    }

    // 初始状态历史
    sqlx::query(
        "INSERT INTO order_status_history (order_id, from_status, to_status, changed_by) VALUES ($1,$2,$3,$4)"
    )
    .bind(rec.order_id)
    .bind::<Option<OrderStatusEnum>>(None)
    .bind(OrderStatusEnum::PENDING)
    .bind(user_token.user_id as i64)
    .execute(&mut *tx)
    .await?;

    // 读取条目并附加食品信息
    let items_out: Vec<OrderItemOut> = sqlx::query(
        "SELECT oi.id, oi.food_id, oi.quantity, oi.price, f.food_name, f.food_photo \
         FROM order_items oi LEFT JOIN foods f ON f.food_id = oi.food_id WHERE oi.order_id=$1"
    )
    .bind(rec.order_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| OrderItemOut {
        id: r.get("id"),
        food_id: r.get("food_id"),
        food_name: r.try_get::<String, _>("food_name").ok(),
        food_photo: r.try_get::<Option<String>, _>("food_photo").ok().flatten(),
        quantity: r.get("quantity"),
        price: r.try_get("price").ok(),
    })
    .collect();

    let history_rows: Vec<OrderStatusHistoryOut> = sqlx::query(
        "SELECT h.from_status, h.to_status, u.nick_name, h.remark, h.changed_at \
         FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
         WHERE h.order_id=$1 ORDER BY h.changed_at"
    )
    .bind(rec.order_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| {
        let from_s = row.get::<Option<String>, _>("from_status").and_then(|s| match s.as_str() {
            "PENDING" => Some(OrderStatusEnum::PENDING),
            "ACCEPTED" => Some(OrderStatusEnum::ACCEPTED),
            "FINISHED" => Some(OrderStatusEnum::FINISHED),
            "CANCELLED" => Some(OrderStatusEnum::CANCELLED),
            "EXPIRED" => Some(OrderStatusEnum::EXPIRED),
            "REJECTED" => Some(OrderStatusEnum::REJECTED),
            "SYSTEM_CLOSED" => Some(OrderStatusEnum::SYSTEM_CLOSED),
            _ => None,
        });
        let to_s = match row.get::<String, _>("to_status").as_str() {
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
            remark: row.try_get("remark").ok(),
            changed_at: row.get("changed_at"),
        }
    })
    .collect();

    tx.commit().await?;

    // 异步推送
    {
        let pool_clone = state.db_pool.clone();
        let oid = rec.order_id;
        tokio::spawn(async move {
            if let Err(e) = crate::services::notifications::push_order_status(oid, pool_clone).await {
                log::warn!("order create push error: {}", e);
            }
        });
    }

    Ok(HttpResponse::Created().json(&OrderOutNew::from((rec, items_out, history_rows))))
}
