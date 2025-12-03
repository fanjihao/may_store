use std::{collections::HashMap, sync::Arc};

use crate::{
    errors::CustomError,
    models::{
        dashboard::{
            LotteryDto, LotteryItemOut, OrderCollectOut, OrderRankingDto, TodayOrderOut,
            TodayPointsOut,
        },
        foods::FoodApplyStruct,
    },
    AppState,
};
use ntex::web::types::{Json, Query, State};
use sqlx::Row;

#[utoipa::path(
    get,
    path = "/dashboard/ranking",
    tag = "用户",
    responses(
        (status = 200, body = Vec<FoodApplyStruct>),
        (status = 400, body = CustomError)
    )
)]
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

#[utoipa::path(
    get,
    path = "/dashboard/collect",
    tag = "用户",
    responses(
        (status = 200, body = OrderCollectOut),
        (status = 400, body = CustomError)
    )
)]
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

#[utoipa::path(
    get,
    path = "/dashboard/today-order",
    tag = "用户",
    responses(
        (status = 200, body = Vec<TodayOrderOut>),
        (status = 400, body = CustomError)
    )
)]
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

#[utoipa::path(
    get,
    path = "/dashboard/today-points",
    tag = "用户",
    responses(
        (status = 200, body = TodayPointsOut),
        (status = 400, body = CustomError)
    )
)]
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

    Ok(Json(TodayPointsOut {
        deduct_sum: row.deduct_sum,
        earn_sum: row.earn_sum,
        today_sum: row.today_sum,
    }))
}

#[utoipa::path(
    post,
    path = "/dashboard/lottery",
    tag = "用户",
    responses(
        (status = 200, body = Vec<LotteryItemOut>),
        (status = 400, body = CustomError)
    )
)]
pub async fn lottery(
    data: Json<LotteryDto>,
    state: State<Arc<AppState>>,
) -> Result<Json<Vec<LotteryItemOut>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    if data.food_types.is_empty() {
        return Ok(Json(vec![]));
    }

    // 使用 UNNEST + DISTINCT ON 方案一次性获取每个类型的随机一条记录
    // 兼容 foods 表字段:
    //   food_types (类型), is_del 逻辑删除(假设 1=删除), user_id 归属
    // 逻辑: 对于传入的 food_types 列表, 在 foods 中按 food_types 和 user_id 过滤, 每类型随机排序取第一条
    let rows = sqlx::query(
        r#"
        WITH requested_types AS (
            SELECT UNNEST($2::int[]) AS food_type
        )
        SELECT DISTINCT ON (rt.food_type)
            rt.food_type AS sel_food_type,
            f.food_id,
            f.food_name,
            f.food_photo,
            f.food_tags,
            f.food_types,
            f.food_status,
            f.food_reason,
            f.create_time,
            f.finish_time,
            f.is_mark,
            f.is_del,
            f.user_id,
            f.apply_remarks
        FROM requested_types rt
        LEFT JOIN LATERAL (
            SELECT * FROM foods f
            WHERE f.food_types = rt.food_type
              AND f.user_id = $1
              AND (f.is_del IS NULL OR f.is_del = 0)
            ORDER BY RANDOM()
            LIMIT 1
        ) f ON TRUE
        ORDER BY rt.food_type, RANDOM();
        "#,
    )
    .bind(data.user_id)
    .bind(&data.food_types)
    .fetch_all(db_pool)
    .await?;

    use std::collections::HashMap as StdHashMap;
    let mut map: StdHashMap<i32, Option<FoodApplyStruct>> = StdHashMap::new();
    for row in rows.iter() {
        let food_type: Option<i32> = row.get("sel_food_type");
        let food_id: Option<i32> = row.get("food_id");
        if let Some(food_type) = food_type {
            if let Some(food_id) = food_id {
                map.insert(
                    food_type,
                    Some(FoodApplyStruct {
                        food_id: Some(food_id),
                        food_name: row
                            .get::<Option<String>, _>("food_name")
                            .unwrap_or_default(),
                        food_photo: row.get("food_photo"),
                        food_tags: row.get("food_tags"),
                        food_types: row.get("food_types"),
                        class_name: None,
                        food_status: row.get("food_status"),
                        food_reason: row.get("food_reason"),
                        create_time: row.get("create_time"),
                        finish_time: row.get("finish_time"),
                        is_mark: row.get("is_mark"),
                        is_del: row.get("is_del"),
                        user_id: row.get("user_id"),
                        apply_remarks: row.get("apply_remarks"),
                        last_order_time: None,
                        last_complete_time: None,
                        total_order_count: None,
                        completed_order_count: None,
                    }),
                );
            } else {
                map.insert(food_type, None);
            }
        }
    }

    // 保持输入顺序输出
    let mut resp: Vec<LotteryItemOut> = Vec::with_capacity(data.food_types.len());
    for ft in data.food_types.iter() {
        if let Some(food) = map.get(ft) {
            resp.push(LotteryItemOut {
                food_type: *ft,
                food: food.clone(),
            });
        } else {
            resp.push(LotteryItemOut {
                food_type: *ft,
                food: None,
            });
        }
    }

    Ok(Json(resp))
}
