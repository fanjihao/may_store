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

    let points_cost = data.points_cost.unwrap_or(0);
    let points_reward = data.points_reward.unwrap_or(0); // 初始奖励可后续调整

    let rec = sqlx::query(
        "INSERT INTO orders (user_id, receiver_id, group_id, goal_time, points_cost, points_reward) VALUES ($1,$2,$3,$4,$5,$6) RETURNING order_id, user_id, receiver_id, group_id, status, goal_time, points_cost, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at"
    )
    .bind(user_token.user_id as i64)
    .bind(data.receiver_id)
    .bind(data.group_id)
    .bind(data.goal_time)
    .bind(points_cost)
    .bind(points_reward)
    .fetch_one(&mut *tx)
    .await?;
    let rec = OrderRecord {
        order_id: rec.get("order_id"),
        user_id: rec.get("user_id"),
        receiver_id: rec.get("receiver_id"),
        group_id: rec.get("group_id"),
        status: match rec.get::<String, _>("status").as_str() {
            "PENDING" => OrderStatusEnum::PENDING,
            "ACCEPTED" => OrderStatusEnum::ACCEPTED,
            "FINISHED" => OrderStatusEnum::FINISHED,
            "CANCELLED" => OrderStatusEnum::CANCELLED,
            "EXPIRED" => OrderStatusEnum::EXPIRED,
            "REJECTED" => OrderStatusEnum::REJECTED,
            "SYSTEM_CLOSED" => OrderStatusEnum::SYSTEM_CLOSED,
            _ => OrderStatusEnum::PENDING,
        },
        goal_time: rec.try_get("goal_time").ok(),
        points_cost: rec.get("points_cost"),
        points_reward: rec.get("points_reward"),
        cancel_reason: rec.try_get("cancel_reason").ok(),
        reject_reason: rec.try_get("reject_reason").ok(),
        last_status_change_at: rec.try_get("last_status_change_at").ok(),
        created_at: rec.get("created_at"),
        updated_at: rec.get("updated_at"),
    };

    // 插入 items
    for item in &data.items {
        let qty = item.quantity.unwrap_or(1).max(1); // 至少 1
        sqlx::query("INSERT INTO order_items (order_id, food_id, quantity) VALUES ($1,$2,$3)")
            .bind(rec.order_id)
            .bind(item.food_id)
            .bind(qty)
            .execute(&mut *tx)
            .await?;
    }

    // 初始状态历史（from NULL -> PENDING）
    sqlx::query("INSERT INTO order_status_history (order_id, from_status, to_status, changed_by, remark) VALUES ($1,$2,$3,$4,$5)")
        .bind(rec.order_id)
        .bind::<Option<String>>(None)
        .bind("PENDING")
        .bind(user_token.user_id as i64)
        .bind::<Option<String>>(None)
        .execute(&mut *tx)
        .await?;

    // 组装输出
    let item_rows: Vec<OrderItemRecord> = sqlx::query(
        "SELECT id, order_id, food_id, quantity, price, snapshot_json, created_at FROM order_items WHERE order_id=$1"
    )
    .bind(rec.order_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| OrderItemRecord {
        id: r.get("id"),
        order_id: r.get("order_id"),
        food_id: r.get("food_id"),
        quantity: r.get("quantity"),
        price: r.try_get("price").ok(),
        snapshot_json: r.try_get("snapshot_json").ok(),
        created_at: r.get("created_at"),
    })
    .collect();

    // 食品快照
    let mut items_out: Vec<OrderItemOut> = Vec::new();
    for ir in item_rows {
        let food = sqlx::query("SELECT food_name, food_photo FROM foods WHERE food_id=$1")
            .bind(ir.food_id)
            .fetch_optional(&mut *tx)
            .await?;
        let (name_opt, photo_opt) = food
            .map(|r| {
                (
                    r.get::<String, _>("food_name"),
                    r.get::<Option<String>, _>("food_photo"),
                )
            })
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

    let history_rows: Vec<OrderStatusHistoryOut> = sqlx::query(
        "SELECT from_status::text, to_status::text, changed_by, remark, changed_at FROM order_status_history WHERE order_id=$1 ORDER BY changed_at"
    )
    .bind(rec.order_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|row| {
        let from_s = row
            .get::<Option<String>, _>("from_status")
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
            changed_by: row.get::<Option<i64>, _>("changed_by"),
            remark: row.get::<Option<String>, _>("remark"),
            changed_at: row.get::<DateTime<Utc>, _>("changed_at"),
        }
    })
    .collect();

    tx.commit().await?;

    // 异步推送订单创建状态（PENDING）
    {
        let pool_clone = state.db_pool.clone();
        let oid = rec.order_id;
        tokio::spawn(async move {
            if let Err(e) = crate::services::notifications::push_order_status(oid, pool_clone).await {
                log::warn!("order create push error: {}", e);
            }
        });
    }

    let out = OrderOutNew::from((rec, items_out, history_rows));
    Ok(HttpResponse::Created().json(&out))
}
