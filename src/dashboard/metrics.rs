use crate::{errors::CustomError, models::dashboard::{DateFoodOut, DateFoodsResponse, DateQuery, JourneyOrderOut, OrderStatsOut, PointsJourneyOut, TodayOrderEntryOut, TodayOrdersResponse, TopFoodOrderOut, TopFoodRankingResponse, WeekDateInfo, WeekOrderDatesOut}, models::users::UserToken, AppState};
use chrono::{Datelike, Local, NaiveDate};
use ntex::web::{
    types::{Query, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

// TopFoodOrderOut, TopFoodRankingResponse, etc. are now in models::dashboard

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

// TodayOrderEntryOut and TodayOrdersResponse are now in models::dashboard

#[utoipa::path(get, path="/dashboard/my/orders-today", tag="看板", responses((status=200, body=TodayOrdersResponse)), security(("cookie_auth"=[])))]
pub async fn get_my_today_orders(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let rows = sqlx::query("SELECT o.order_id, o.status AS status, ARRAY_AGG(f.food_name) AS names, MIN(t.tag_name) AS tag_name FROM orders o JOIN order_items oi ON o.order_id=oi.order_id JOIN foods f ON oi.food_id=f.food_id LEFT JOIN tags t ON f.tag_id=t.tag_id WHERE o.user_id=$1 AND o.goal_time IS NOT NULL AND o.goal_time::date=CURRENT_DATE AND o.status IN ('PENDING','ACCEPTED','FINISHED') GROUP BY o.order_id, o.status")
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
            let category: String = r.get::<Option<String>, _>("tag_name").unwrap_or("其他".to_string());
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

// OrderStatsOut is now in models::dashboard

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

// JourneyOrderOut and PointsJourneyOut are now in models::dashboard

#[utoipa::path(get, path="/dashboard/my/points-journey", tag="看板", responses((status=200, body=PointsJourneyOut)), security(("cookie_auth"=[])))]
pub async fn get_points_journey(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 今日待办订单（PENDING/ACCEPTED）
    let order_rows = sqlx::query("SELECT o.order_id, o.status AS status, ARRAY_AGG(f.food_name) AS names FROM orders o JOIN order_items oi ON o.order_id=oi.order_id JOIN foods f ON oi.food_id=f.food_id WHERE o.user_id=$1 AND o.goal_time IS NOT NULL AND o.goal_time::date=CURRENT_DATE AND o.status IN ('PENDING','ACCEPTED') GROUP BY o.order_id, o.status")
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

// WeekOrderDatesOut and WeekDateInfo are now in models::dashboard

#[utoipa::path(
    get,
    path = "/dashboard/week-order-dates",
    tag = "看板",
    responses((status = 200, body = WeekOrderDatesOut)),
    security(("cookie_auth" = []))
)]
pub async fn get_week_order_dates(
    state: State<Arc<AppState>>,
    user: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    // 获取本周的周一
    let today = Local::now().date_naive();
    let day_of_week = today.weekday().num_days_from_monday(); // 0=周一, 6=周日
    let monday = today
        .checked_sub_days(chrono::Days::new(day_of_week as u64))
        .unwrap_or(today);

    // 查询本周有订单的日期
    use chrono::{TimeZone, Utc};
    let monday_at_time = Utc.with_ymd_and_hms(monday.year(), monday.month(), monday.day(), 0, 0, 0).unwrap();
    let order_dates = sqlx::query!(
        r#"
        SELECT DATE(goal_time)::date as order_date, COUNT(*)::int as cnt
        FROM orders
        WHERE user_id = $1
          AND goal_time >= $2
          AND goal_time < $2 + INTERVAL '7 days'
        GROUP BY DATE(goal_time)
        "#,
        user.user_id as i64,
        monday_at_time
    )
    .fetch_all(db)
    .await?;

    let order_date_set: std::collections::HashSet<NaiveDate> = order_dates
        .iter()
        .filter_map(|r| r.order_date)
        .collect();

    let mut week_dates: Vec<WeekDateInfo> = Vec::new();
    for i in 0..7 {
        if let Some(date) = monday.checked_add_days(chrono::Days::new(i)) {
            let has_order = order_date_set.contains(&date);
            let count = if has_order {
                order_dates
                    .iter()
                    .find(|r| r.order_date == Some(date))
                    .and_then(|r| r.cnt)
                    .unwrap_or(0)
            } else {
                0
            };
            week_dates.push(WeekDateInfo {
                date,
                day_of_week: i as i32 + 1, // 1-7
                has_order,
                order_count: count,
            });
        }
    }

    Ok(HttpResponse::Ok().json(&WeekOrderDatesOut {
        week_dates,
        message: None,
    }))
}

// DateFoodOut, DateFoodsResponse, and DateQuery are now in models::dashboard

#[utoipa::path(
    get,
    path = "/dashboard/date-foods",
    tag = "看板",
    params(DateQuery),
    responses((status = 200, body = DateFoodsResponse)),
    security(("cookie_auth" = []))
)]
pub async fn get_date_foods(
    state: State<Arc<AppState>>,
    user: UserToken,
    query: Query<DateQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    // 如果没有提供日期，默认今天
    let target_date = query.date.unwrap_or_else(|| Local::now().date_naive());

    // 查询当天订单中的菜品（去重）
    let rows = sqlx::query(
        r#"
        SELECT
            f.food_id,
            f.food_name,
            f.food_photo,
            f.ingredients,
            f.steps,
            t.tag_name
        FROM orders o
        JOIN order_items oi ON o.order_id = oi.order_id
        JOIN foods f ON oi.food_id = f.food_id
        LEFT JOIN tags t ON f.tag_id = t.tag_id
        WHERE o.user_id = $1
          AND DATE(o.goal_time) = $2
        GROUP BY f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, t.tag_name
        ORDER BY f.food_id
        "#,
    )
    .bind(user.user_id as i64)
    .bind(target_date)
    .fetch_all(db)
    .await?;

    let foods_list: Vec<DateFoodOut> = rows
        .into_iter()
        .map(|r| DateFoodOut {
            food_id: r.get("food_id"),
            food_name: r.get("food_name"),
            food_photo: r.try_get("food_photo").ok(),
            ingredients: r.try_get("ingredients").ok(),
            steps: r.try_get("steps").ok(),
            tag_name: r.try_get("tag_name").ok(),
        })
        .collect();

    let message = if foods_list.is_empty() {
        Some("当天暂无订单数据".into())
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(&DateFoodsResponse {
        date: target_date,
        foods: foods_list,
        message,
    }))
}
