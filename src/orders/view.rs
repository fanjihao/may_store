use std::{collections::HashMap, sync::Arc};

use ntex::web::{types::{Json, Path, Query, State}, Responder};

use crate::{
    errors::CustomError,
    models::orders::{OrderDetailListOut, OrderListDto, OrderListOut, OrderOut},
    AppState,
};

#[utoipa::path(
    get,
    path = "/orders",
    params(
        ("status", Query, description = "订单状态"),
        ("user_id", Query, description = "用户ID")
    ),
    tag = "订单",
    responses(
        (status = 200, body = Vec<OrderListOut>, description = "获取订单列表")
    )
)]
pub async fn get_orders(
    state: State<Arc<AppState>>,
    data: Query<OrderListDto>,
) -> Result<Json<Vec<OrderListOut>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let rows = sqlx::query!(
        "SELECT o.*, od.food_id , od.order_d_id, od.user_id, f.food_name, f.food_photo
         FROM orders o
         LEFT JOIN orders_d od ON o.order_id = od.order_id
	     LEFT JOIN foods f ON f.food_id = od.food_id 
         WHERE (o.recv_user_id = $1 OR o.create_user_id = $1)
         AND (o.order_status = ANY(string_to_array($2, ',')::int[]) OR $2 IS NULL)
         AND o.is_del = 0 ORDER BY create_date DESC;",
        data.user_id,
        data.status
    )
    .fetch_all(db_pool)
    .await?;

    // 使用 HashMap 进行分组，将每个 order_id 的子项合并
    let mut orders_map: HashMap<i32, OrderListOut> = HashMap::new();

    for row in rows {
        let order_id = row.order_id;

        // 如果订单不在 map 中，先插入订单信息
        let order_entry = orders_map.entry(order_id).or_insert_with(|| OrderListOut {
            order_id: Some(row.order_id),
            order_no: row.order_no,
            order_status: row.order_status,
            create_date: row.create_date,
            create_user_id: row.create_user_id,
            recv_user_id: row.recv_user_id,
            goal_time: row.goal_time,
            finish_time: row.finish_time,
            remarks: row.remarks,
            is_del: row.is_del,
            order_detail: Some(Vec::new()),
        });

        if let Some(order_detail) = &mut order_entry.order_detail {
            order_detail.push(OrderDetailListOut {
                order_d_id: Some(row.order_d_id),
                order_id: Some(order_id),
                food_id: row.food_id,
                food_name: row.food_name,
                food_photo: row.food_photo,
                user_id: row.user_id,
            });
        }
    }

    // 提取 HashMap 的值作为结果返回
    let mut order_list: Vec<OrderListOut> = orders_map.into_values().collect();
    order_list.sort_by(|a, b| b.create_date.cmp(&a.create_date));

    Ok(Json(order_list))
}

#[utoipa::path(
    get,
    path = "/orders/{id}",
    tag = "订单",
    responses(
        (status = 200, body = OrderOut, description = "获取订单详情")
    )
)]
pub async fn get_order_detail(
    state: State<Arc<AppState>>,
    id: Path<(i32,)>,
) -> Result<Json<OrderOut>, CustomError> {
    let db_pool = &state.clone().db_pool;
    let id = id.0;
    let rows = sqlx::query!(
        "SELECT o.*, od.food_id , od.order_d_id, od.user_id, f.food_name, f.food_photo, p.points
         FROM orders o
         LEFT JOIN orders_d od ON o.order_id = od.order_id
	     LEFT JOIN foods f ON f.food_id = od.food_id 
	     LEFT JOIN points_history p ON p.bind_id = o.order_id
         WHERE o.order_id = $1",
        &id
    )
    .fetch_all(db_pool)
    .await?;

    // 使用 HashMap 进行分组，将每个 order_id 的子项合并
    let mut orders_map: HashMap<i32, OrderOut> = HashMap::new();

    for row in rows {
        let order_id = row.order_id;

        // 如果订单不在 map 中，先插入订单信息
        let order_entry = orders_map.entry(order_id).or_insert_with(|| OrderOut {
            order_id: Some(row.order_id),
            order_no: row.order_no,
            order_status: row.order_status,
            create_date: row.create_date,
            create_user_id: row.create_user_id,
            recv_user_id: row.recv_user_id,
            goal_time: row.goal_time,
            finish_time: row.finish_time,
            remarks: row.remarks,
            is_del: row.is_del,
            revoke_time: row.revoke_time,
            approval_time: row.approval_time,
            approval_feedback: row.approval_feedback,
            finish_feedback: row.finish_feedback,
            approval_status: row.approval_status,
            finish_status: row.finish_status,
            points: row.points,
            order_detail: Some(Vec::new()),
        });

        if let Some(order_detail) = &mut order_entry.order_detail {
            order_detail.push(OrderDetailListOut {
                order_d_id: Some(row.order_d_id),
                order_id: Some(order_id),
                food_id: row.food_id,
                food_name: row.food_name,
                food_photo: row.food_photo,
                user_id: row.user_id,
            });
        }
    }

    // 提取 HashMap 的值作为结果返回
    let order_list: Vec<OrderOut> = orders_map.into_values().collect();

    if let Some(order) = order_list.into_iter().next() {
        Ok(Json(order))
    } else {
        Err(CustomError::BadRequest("获取详情失败".to_string()))
    }
}

#[utoipa::path(
    get,
    path = "/orders/incomplete/{id}",
    tag = "未完成订单",
    responses(
        (status = 200, body = i32, description = "获取订单详情")
    )
)]
pub async fn get_incomplete_order(state: State<Arc<AppState>>, id: Path<(i32,)>) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let result = sqlx::query!("SELECT COUNT(*)   
        FROM orders   
        WHERE order_status NOT IN (0, 1)   
        AND recv_user_id = $1", id.0)
        .fetch_one(db_pool)
        .await?;

    Ok(Json(result.count))
}