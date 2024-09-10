use std::{collections::HashMap, sync::Arc};

use ntex::web::types::{Json, Query, State};

use crate::{
    errors::CustomError,
    models::orders::{OrderDetailListOut, OrderListDto, OrderListOut},
    AppState,
};

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
         AND (o.order_status = COALESCE($2, o.order_status) OR $2 IS NULL)
         AND o.is_del = 0;",
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
            order_c_status: row.order_c_status,
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
    let order_list = orders_map.into_values().collect();

    Ok(Json(order_list))
}
