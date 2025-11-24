use crate::{errors::CustomError, models::users::UserToken, AppState};
use ntex::web::{
    types::State,
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

// ============== Top Ordered Foods Ranking ==============
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TopFoodOrderOut {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: String,
    pub order_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TopFoodRankingResponse {
    pub list: Vec<TopFoodOrderOut>,
    pub message: Option<String>,
}

#[utoipa::path(
    get, 
    path="/dashboard/top-foods", 
    tag="看板", 
    responses((status=200, body=TopFoodRankingResponse)), 
    security(("cookie_auth"=[]))
)]
pub async fn get_top_food_orders(
    state: State<Arc<AppState>>,
    _user: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 统计前五
    let rows = sqlx::query("SELECT oi.food_id, f.food_name, f.food_photo, COUNT(*)::bigint AS order_count FROM order_items oi JOIN orders o ON oi.order_id=o.order_id JOIN foods f ON oi.food_id=f.food_id GROUP BY oi.food_id, f.food_name, f.food_photo ORDER BY order_count DESC LIMIT 5")
        .fetch_all(db).await?;
    if rows.is_empty() {
        // 没有订单：随机抽取菜品
        let random_rows =
            sqlx::query("SELECT food_id, food_name, food_photo FROM foods ORDER BY random() LIMIT 5")
                .fetch_all(db)
                .await?;
        if random_rows.is_empty() {
            return Ok(HttpResponse::Ok().json(&TopFoodRankingResponse {
                list: vec![],
                message: Some("暂无数据".into()),
            }));
        }
        let list: Vec<TopFoodOrderOut> = random_rows
            .into_iter()
            .map(|r| TopFoodOrderOut {
                food_id: r.get("food_id"),
                food_name: r.get("food_name"),
                food_photo: r.get("food_photo"),
                order_count: 0,
            })
            .collect();
        return Ok(HttpResponse::Ok().json(&TopFoodRankingResponse {
            list,
            message: Some("无订单数据，随机推荐".into()),
        }));
    }
    let list: Vec<TopFoodOrderOut> = rows
        .into_iter()
        .map(|r| TopFoodOrderOut {
            food_id: r.get("food_id"),
            food_name: r.get("food_name"),
            food_photo: r.get("food_photo"),
            order_count: r.get::<i64, _>("order_count"),
        })
        .collect();
    Ok(HttpResponse::Ok().json(&TopFoodRankingResponse {
        list,
        message: None,
    }))
}

// ============== Today's Orders Tree ==============
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TodayOrderEntryOut {
    pub order_id: i64,
    pub category: String,
    pub foods_text: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TodayOrdersResponse {
    pub list: Vec<TodayOrderEntryOut>,
    pub message: Option<String>,
}

#[utoipa::path(get, path="/dashboard/my/orders-today", tag="看板", responses((status=200, body=TodayOrdersResponse)), security(("cookie_auth"=[])))]
pub async fn get_my_today_orders(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let rows = sqlx::query("SELECT o.order_id, o.status::text AS status, ARRAY_AGG(f.food_name) AS names, MIN(f.food_types) AS category_code FROM orders o JOIN order_items oi ON o.order_id=oi.order_id JOIN foods f ON oi.food_id=f.food_id WHERE o.user_id=$1 AND o.goal_time IS NOT NULL AND o.goal_time::date=CURRENT_DATE AND o.status IN ('PENDING','ACCEPTED','FINISHED') GROUP BY o.order_id, o.status")
        .bind(user.user_id)
        .fetch_all(db).await?;
    if rows.is_empty() {
        return Ok(HttpResponse::Ok().json(&TodayOrdersResponse {
            list: vec![],
            message: Some("暂无订单~".into()),
        }));
    }
    let mut entries: Vec<TodayOrderEntryOut> = rows
        .into_iter()
        .map(|r| {
            let code: i32 = r.get("category_code");
            let category = match code {
                1 => "早上",
                2 => "中午",
                3 => "下午",
                4 => "晚上",
                _ => "其他",
            }
            .to_string();
            // ARRAY_AGG returns Value; attempt to treat as Vec<String>
            let names_val: serde_json::Value = r.get("names");
            let foods_text = names_val
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("+")
                })
                .unwrap_or_default();
            TodayOrderEntryOut {
                order_id: r.get("order_id"),
                category,
                foods_text,
                status: r.get("status"),
            }
        })
        .collect();
    // 排序：category 顺序 早上->中午->下午->晚上
    let order_rank = |c: &str| match c {
        "早上" => 1,
        "中午" => 2,
        "下午" => 3,
        "晚上" => 4,
        _ => 99,
    };
    entries.sort_by_key(|e| order_rank(&e.category));
    Ok(HttpResponse::Ok().json(&TodayOrdersResponse {
        list: entries,
        message: None,
    }))
}

// ============== Order Stats ==============
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OrderStatsOut {
    pub total_orders: i64,
    pub finished_orders: i64,
    pub rejected_orders: i64,
}

#[utoipa::path(get, path="/dashboard/my/order-stats", tag="看板", responses((status=200, body=OrderStatsOut)), security(("cookie_auth"=[])))]
pub async fn get_my_order_stats(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let total_row = sqlx::query("SELECT COUNT(*)::bigint AS c FROM orders WHERE user_id=$1")
        .bind(user.user_id)
        .fetch_one(db)
        .await?;
    let finished_row = sqlx::query(
        "SELECT COUNT(*)::bigint AS c FROM orders WHERE user_id=$1 AND status='FINISHED'",
    )
    .bind(user.user_id)
    .fetch_one(db)
    .await?;
    let rejected_row = sqlx::query(
        "SELECT COUNT(*)::bigint AS c FROM orders WHERE user_id=$1 AND status='REJECTED'",
    )
    .bind(user.user_id)
    .fetch_one(db)
    .await?;
    let out = OrderStatsOut {
        total_orders: total_row.get("c"),
        finished_orders: finished_row.get("c"),
        rejected_orders: rejected_row.get("c"),
    };
    Ok(HttpResponse::Ok().json(&out))
}

// ============== Points Journey ==============
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JourneyOrderOut {
    pub order_id: i64,
    pub foods_text: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PointsJourneyOut {
    pub today_orders: Vec<JourneyOrderOut>,
    pub today_points: i64,
    pub current_points: i32,
    pub total_gain_points: i64,
    pub total_cost_points: i64,
    pub message: Option<String>,
}

#[utoipa::path(get, path="/dashboard/my/points-journey", tag="看板", responses((status=200, body=PointsJourneyOut)), security(("cookie_auth"=[])))]
pub async fn get_points_journey(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 今日待办订单（PENDING/ACCEPTED）
    let order_rows = sqlx::query("SELECT o.order_id, o.status::text AS status, ARRAY_AGG(f.food_name) AS names FROM orders o JOIN order_items oi ON o.order_id=oi.order_id JOIN foods f ON oi.food_id=f.food_id WHERE o.user_id=$1 AND o.goal_time IS NOT NULL AND o.goal_time::date=CURRENT_DATE AND o.status IN ('PENDING','ACCEPTED') GROUP BY o.order_id, o.status")
        .bind(user.user_id).fetch_all(db).await?;
    let journey_orders: Vec<JourneyOrderOut> = order_rows
        .into_iter()
        .map(|r| {
            let names_val: serde_json::Value = r.get("names");
            let foods_text = names_val
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("+")
                })
                .unwrap_or_default();
            JourneyOrderOut {
                order_id: r.get("order_id"),
                foods_text,
                status: r.get("status"),
            }
        })
        .collect();
    // 积分统计
    let today_points_row = sqlx::query("SELECT COALESCE(SUM(amount),0)::bigint AS s FROM point_transactions WHERE user_id=$1 AND amount>0 AND created_at::date=CURRENT_DATE")
        .bind(user.user_id).fetch_one(db).await?;
    let total_gain_row = sqlx::query("SELECT COALESCE(SUM(amount),0)::bigint AS s FROM point_transactions WHERE user_id=$1 AND amount>0")
        .bind(user.user_id).fetch_one(db).await?;
    let total_cost_row = sqlx::query("SELECT COALESCE(SUM(-amount),0)::bigint AS s FROM point_transactions WHERE user_id=$1 AND amount<0")
        .bind(user.user_id).fetch_one(db).await?;
    let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1")
        .bind(user.user_id)
        .fetch_one(db)
        .await?;
    let out = PointsJourneyOut {
        today_orders: journey_orders.clone(),
        today_points: today_points_row.get("s"),
        current_points: user_row.get("love_point"),
        total_gain_points: total_gain_row.get("s"),
        total_cost_points: total_cost_row.get("s"),
        message: if journey_orders.is_empty() {
            Some("暂无数据~".into())
        } else {
            None
        },
    };
    Ok(HttpResponse::Ok().json(&out))
}
