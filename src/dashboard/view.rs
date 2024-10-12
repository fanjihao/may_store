use std::{collections::HashMap, sync::Arc};

use crate::{
    errors::CustomError,
    models::{
        dashboard::{OrderCollectOut, OrderRankingDto, TodayOrderOut, TodayPointsOut},
        foods::FoodApplyStruct,
    },
    AppState,
};
use ntex::web::types::{Json, Query, State};
use sqlx::Row;

pub async fn order_ranking(
    data: Query<OrderRankingDto>,
    state: State<Arc<AppState>>,
) -> Result<Json<Vec<FoodApplyStruct>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let mut sql_str = format!(
        "SELECT
            f.*,
            MAX ( o.create_date ) AS last_order_time,
            MAX ( o.finish_time ) AS last_complete_time,
            COUNT ( od.order_id ) AS total_order_count,
            SUM ( CASE WHEN o.order_status = 3 THEN 1 ELSE 0 END ) AS completed_order_count 
        FROM
            foods f
            LEFT JOIN orders_d od ON f.food_id = od.food_id
            LEFT JOIN orders o ON od.order_id = o.order_id "
    );
    match data.role {
        Some(role) => {
            if role == 0 {
                sql_str = format!("{}WHERE o.recv_user_id = $1 ", sql_str);
            } else {
                sql_str = format!("{}WHERE o.create_user_id = $1 ", sql_str);
            }

            sql_str = format!(
                "{}GROUP BY f.food_id ORDER BY total_order_count DESC LIMIT 5;",
                sql_str
            );

            let foods = sqlx::query(&sql_str)
                .bind(data.user_id)
                .fetch_all(db_pool)
                .await?
                .into_iter()
                .map(|row| FoodApplyStruct {
                    food_id: row.get("food_id"),
                    food_name: row.get("food_name"),
                    food_photo: row.get("food_photo"),
                    class_name: None,
                    food_reason: row.get("food_reason"),
                    create_time: row.get("create_time"),
                    finish_time: row.get("finish_time"),
                    is_mark: row.get("is_mark"),
                    is_del: row.get("is_del"),
                    user_id: row.get("user_id"),
                    food_status: row.get("food_status"),
                    food_types: row.get("food_types"),
                    food_tags: row.get("food_tags"),
                    apply_remarks: row.get("apply_remarks"),
                    last_order_time: row.get("last_order_time"),
                    last_complete_time: row.get("last_complete_time"),
                    total_order_count: row.get("total_order_count"),
                    completed_order_count: row.get("completed_order_count"),
                })
                .collect();

            Ok(Json(foods))
        }
        None => Err(CustomError::BadRequest("缺少参数 role".to_string())),
    }
}

pub async fn order_collect(
    data: Query<OrderRankingDto>,
    state: State<Arc<AppState>>,
) -> Result<Json<OrderCollectOut>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let result = sqlx::query!(
        "SELECT COUNT
            ( * ) AS total_orders,
            COUNT ( CASE WHEN order_status = 3 THEN 1 END ) AS completed_orders,
            COUNT ( CASE WHEN order_status = 7 THEN 1 END ) AS rejected_orders 
        FROM
            orders
            WHERE create_user_id = $1;",
        data.user_id
    )
    .fetch_one(db_pool)
    .await?;

    Ok(Json(OrderCollectOut {
        total_orders: result.total_orders,
        completed_orders: result.completed_orders,
        rejected_orders: result.rejected_orders,
    }))
}

pub async fn today_order(
    data: Query<OrderRankingDto>,
    state: State<Arc<AppState>>,
) -> Result<Json<Vec<TodayOrderOut>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let mut sql_str = format!(
        "SELECT
            o.order_id,
            o.goal_time,
            od.food_id,
            f.food_name,
            f.food_photo 
        FROM
            orders o
            LEFT JOIN orders_d od ON o.order_id = od.order_id
            LEFT JOIN foods f ON f.food_id = od.food_id 
        WHERE "
    );

    match data.role {
        Some(role) => {
            if role == 0 {
                sql_str = format!("{}o.recv_user_id = $1", sql_str);
            } else {
                sql_str = format!("{}o.create_user_id = $1", sql_str);
            }
            sql_str = format!(
                "{} AND goal_time::DATE = CURRENT_DATE AND goal_time::TIME > CURRENT_TIME;",
                sql_str
            );

            let rows = sqlx::query(&sql_str)
                .bind(data.user_id)
                .fetch_all(db_pool)
                .await?;

            let mut todays_map: HashMap<i32, TodayOrderOut> = HashMap::new();

            for row in rows {
                let order_id = row.get("order_id");
                let new_food_name: String = row.get("food_name");

                if let Some(order_entry) = todays_map.get_mut(&order_id) {
                    // 键存在，修改对应的值
                    if let Some(food_name) = order_entry.food_name.as_mut() {
                        food_name.push_str(" + ");
                        food_name.push_str(&new_food_name);
                    }
                } else {
                    todays_map.insert(
                        order_id,
                        TodayOrderOut {
                            order_id: row.get("order_id"),
                            goal_time: row.get("goal_time"),
                            food_id: row.get("food_id"),
                            food_name: row.get("food_name"),
                            food_photo: row.get("food_photo"),
                        },
                    );
                }
            }
            // 提取 HashMap 的值作为结果返回
            let today_list: Vec<TodayOrderOut> = todays_map.into_values().collect();

            Ok(Json(today_list))
        }
        None => Err(CustomError::BadRequest("缺少参数 role".to_string())),
    }
}

pub async fn today_points(
    data: Query<OrderRankingDto>,
    state: State<Arc<AppState>>,
) -> Result<Json<TodayPointsOut>, CustomError> {
    let db_pool = &state.clone().db_pool;
    
    let row = sqlx::query!(
        "SELECT SUM
            ( CASE WHEN transaction_type = 'deduct' THEN points ELSE 0 END ) AS deduct_sum,
            SUM ( CASE WHEN transaction_type = 'earn' THEN points ELSE 0 END ) AS earn_sum,
            ( SELECT SUM ( points ) FROM points_history WHERE user_id = $1 AND create_time :: DATE = CURRENT_DATE AND transaction_type IN ( 'earn', 'deduct' ) ) AS today_sum 
        FROM
            points_history 
        WHERE
            user_id = $1;",
        data.user_id
    ).fetch_one(db_pool).await?;

    Ok(Json(TodayPointsOut { deduct_sum: row.deduct_sum, earn_sum: row.earn_sum, today_sum: row.today_sum }))
}
