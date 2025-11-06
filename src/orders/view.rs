use crate::models::users::UserToken;
use crate::{
    errors::CustomError,
    models::orders::{
        OrderItemOut, OrderOutNew, OrderQuery, OrderRecord, OrderStatusEnum, OrderStatusHistoryOut,
    },
    AppState,
};
use chrono::{DateTime, Utc};
use ntex::web::{
    types::{Path, Query, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
	get,
	path = "/orders",
	tag = "订单",
	params(OrderQuery),
	responses((status = 200, body = [OrderOutNew]))
)]
pub async fn get_orders(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<OrderQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 使用 QueryBuilder 动态构建过滤条件
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT order_id, user_id, receiver_id, group_id, status, goal_time, points_cost, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at FROM orders");
    let mut first = true;
    qb.push(" WHERE ");
    if let Some(uid) = query.user_id.or(Some(user_token.user_id)) {
        // default filter user orders unless user explicitly overrides
        if !first {
            qb.push(" AND ");
        } else {
            first = false;
        }
        qb.push(" user_id = ");
        qb.push_bind(uid as i64);
    }
    if let Some(rid) = query.receiver_id {
        if !first {
            qb.push(" AND ");
        } else {
            first = false;
        }
        qb.push(" receiver_id = ");
        qb.push_bind(rid as i64);
    }
    if let Some(st) = query.status {
        if !first {
            qb.push(" AND ");
        } else {
            first = false;
        }
        qb.push(" status = ");
        qb.push_bind(format!("{:?}", st));
    }
    qb.push(" ORDER BY created_at DESC ");
    qb.push(" LIMIT ");
    qb.push_bind(query.limit.unwrap_or(50));
    let rows = qb.build().fetch_all(db).await?;
    let mut out_list: Vec<OrderOutNew> = Vec::new();
    for r in rows {
        let order = map_record(&r);
        // items
        let item_rows = sqlx::query("SELECT id, order_id, food_id, quantity, price, snapshot_json, created_at FROM order_items WHERE order_id=$1")
			.bind(order.order_id)
			.fetch_all(db)
			.await?;
        let items = item_rows
            .into_iter()
            .map(map_item_record_to_out(db))
            .collect::<Result<Vec<_>, _>>()?;
        // status history (last 5 for list)
        let hist_rows = sqlx::query("SELECT from_status::text, to_status::text, changed_by, remark, changed_at FROM order_status_history WHERE order_id=$1 ORDER BY changed_at DESC LIMIT 5")
			.bind(order.order_id)
			.fetch_all(db)
			.await?;
        let history = hist_rows.into_iter().map(map_history_row).collect();
        out_list.push(OrderOutNew::from((order, items, history)));
    }
    Ok(HttpResponse::Ok().json(&out_list))
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
    let db = &state.db_pool;
    let row = sqlx::query("SELECT order_id, user_id, receiver_id, group_id, status, goal_time, points_cost, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at FROM orders WHERE order_id=$1")
		.bind(*id)
		.fetch_optional(db)
		.await?;
    let order_row = match row {
        Some(r) => r,
        None => return Err(CustomError::BadRequest("订单不存在".into())),
    };
    let order = map_record(&order_row);
    let item_rows = sqlx::query("SELECT id, order_id, food_id, quantity, price, snapshot_json, created_at FROM order_items WHERE order_id=$1")
		.bind(order.order_id)
		.fetch_all(db)
		.await?;
    let items = item_rows
        .into_iter()
        .map(map_item_record_to_out(db))
        .collect::<Result<Vec<_>, _>>()?;
    let hist_rows = sqlx::query("SELECT from_status::text, to_status::text, changed_by, remark, changed_at FROM order_status_history WHERE order_id=$1 ORDER BY changed_at")
		.bind(order.order_id)
		.fetch_all(db)
		.await?;
    let history = hist_rows.into_iter().map(map_history_row).collect();
    Ok(HttpResponse::Ok().json(&OrderOutNew::from((order, items, history))))
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
    let db = &state.db_pool;
    let count =
        sqlx::query("SELECT COUNT(*) as c FROM orders WHERE user_id=$1 AND status='PENDING'")
            .bind(*user_id)
            .fetch_one(db)
            .await?;
    let c: i64 = count.get("c");
    Ok(HttpResponse::Ok().json(&(c as i32)))
}

pub fn map_record(row: &sqlx::postgres::PgRow) -> OrderRecord {
    OrderRecord {
        order_id: row.get("order_id"),
        user_id: row.get("user_id"),
        receiver_id: row.get("receiver_id"),
        group_id: row.get("group_id"),
        status: match row.get::<String, _>("status").as_str() {
            "PENDING" => OrderStatusEnum::PENDING,
            "ACCEPTED" => OrderStatusEnum::ACCEPTED,
            "FINISHED" => OrderStatusEnum::FINISHED,
            "CANCELLED" => OrderStatusEnum::CANCELLED,
            "EXPIRED" => OrderStatusEnum::EXPIRED,
            "REJECTED" => OrderStatusEnum::REJECTED,
            "SYSTEM_CLOSED" => OrderStatusEnum::SYSTEM_CLOSED,
            _ => OrderStatusEnum::PENDING,
        },
        goal_time: row.try_get("goal_time").ok(),
        points_cost: row.get("points_cost"),
        points_reward: row.get("points_reward"),
        cancel_reason: row.try_get("cancel_reason").ok(),
        reject_reason: row.try_get("reject_reason").ok(),
        last_status_change_at: row.try_get("last_status_change_at").ok(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub fn map_item_record_to_out<'a>(
    _db: &'a sqlx::Pool<sqlx::Postgres>,
) -> impl Fn(sqlx::postgres::PgRow) -> Result<OrderItemOut, CustomError> + 'a {
    move |r| {
        let food_id: i64 = r.get("food_id");
        Ok(OrderItemOut {
            id: r.get("id"),
            food_id,
            food_name: None, // lazy populate below if needed
            food_photo: None,
            quantity: r.get("quantity"),
            price: r.try_get("price").ok(),
        })
    }
}

pub fn map_history_row(row: sqlx::postgres::PgRow) -> OrderStatusHistoryOut {
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
    OrderStatusHistoryOut {
        from_status: from_s,
        to_status: to_s,
        changed_by: row.get::<Option<i64>, _>("changed_by"),
        remark: row.get::<Option<String>, _>("remark"),
        changed_at: row.get::<DateTime<Utc>, _>("changed_at"),
    }
}
