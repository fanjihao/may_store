// 应用服务层 - 看板服务

use crate::domain::dashboard::*;
use crate::errors::CustomError;
use chrono::{Duration, Local, NaiveDate};
use sqlx::{PgPool, Row};

pub struct DashboardService;

impl DashboardService {
    /// 获取组活动
    pub async fn get_group_activities(
        db: &PgPool,
        group_id: i64,
        query: &GroupActivityQuery,
    ) -> Result<Vec<GroupActivityEventOut>, CustomError> {
        let start_date = query
            .start_date
            .unwrap_or_else(|| Local::now().date_naive() - Duration::days(7));
        let end_date = query.end_date.unwrap_or_else(|| Local::now().date_naive());
        let limit = query.limit.unwrap_or(50).min(100);

        let rows = sqlx::query(
            "SELECT event_type, payload, created_at FROM event_log \
             WHERE group_id = $1 AND created_at >= $2 AND created_at <= $3 \
             ORDER BY created_at DESC LIMIT $4",
        )
        .bind(group_id)
        .bind(start_date)
        .bind(end_date)
        .bind(limit)
        .fetch_all(db)
        .await?;

        let events: Vec<GroupActivityEventOut> = rows
            .into_iter()
            .map(|r| GroupActivityEventOut {
                event_type: r.get("event_type"),
                event_data: r.get("payload"),
                created_at: r.get("created_at"),
            })
            .collect();

        Ok(events)
    }

    /// 获取热门菜品排名
    pub async fn get_top_food_orders(
        db: &PgPool,
        group_id: i64,
    ) -> Result<TopFoodRankingResponse, CustomError> {
        let rows = sqlx::query(
            "SELECT f.food_id, f.name, COUNT(*) as order_count \
             FROM orders o \
             JOIN foods f ON o.food_id = f.food_id \
             WHERE o.group_id = $1 AND o.status = 'COMPLETED' \
             GROUP BY f.food_id, f.name \
             ORDER BY order_count DESC \
             LIMIT 10",
        )
        .bind(group_id)
        .fetch_all(db)
        .await?;

        let rankings: Vec<FoodRanking> = rows
            .into_iter()
            .map(|r| FoodRanking {
                food_id: r.get("food_id"),
                food_name: r.get("name"),
                order_count: r.get("order_count"),
            })
            .collect();

        Ok(TopFoodRankingResponse { rankings })
    }

    /// 获取我今日的订单
    pub async fn get_my_today_orders(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<TodayOrdersResponse, CustomError> {
        let today = Local::now().date_naive();

        let rows = sqlx::query(
            "SELECT o.order_id, o.food_id, o.status, o.created_at, f.name as food_name \
             FROM orders o \
             JOIN foods f ON o.food_id = f.food_id \
             WHERE o.user_id = $1 AND o.group_id = $2 AND DATE(o.created_at) = $3 \
             ORDER BY o.created_at DESC",
        )
        .bind(user_id as i64)
        .bind(group_id)
        .bind(today)
        .fetch_all(db)
        .await?;

        let orders: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "orderId": r.get::<i64, _>("order_id"),
                    "foodId": r.get::<i64, _>("food_id"),
                    "foodName": r.get::<String, _>("food_name"),
                    "status": r.get::<String, _>("status"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect();

        Ok(TodayOrdersResponse { orders })
    }

    /// 获取我的订单统计
    pub async fn get_my_order_stats(
        db: &PgPool,
        user_id: i64,
    ) -> Result<OrderStatsOut, CustomError> {
        let total_orders: i32 = sqlx::query("SELECT COUNT(*) FROM orders WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        let completed_orders: i32 =
            sqlx::query("SELECT COUNT(*) FROM orders WHERE user_id = $1 AND status = 'COMPLETED'")
                .bind(user_id as i64)
                .fetch_one(db)
                .await?
                .get(0);

        let pending_orders: i32 = sqlx::query(
            "SELECT COUNT(*) FROM orders WHERE user_id = $1 AND status IN ('CREATED', 'ACCEPTED')",
        )
        .bind(user_id as i64)
        .fetch_one(db)
        .await?
        .get(0);

        let total_points: i32 = sqlx::query("SELECT love_point FROM users WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        Ok(OrderStatsOut {
            total_orders,
            completed_orders,
            pending_orders,
            total_points,
        })
    }

    /// 获取积分旅程
    pub async fn get_points_journey(
        db: &PgPool,
        user_id: i64,
    ) -> Result<PointsJourneyOut, CustomError> {
        let rows = sqlx::query(
            "SELECT 'order' as event_type, COALESCE(amount, 0) as points, created_at \
             FROM point_flow WHERE user_id = $1 \
             ORDER BY created_at DESC LIMIT 50",
        )
        .bind(user_id as i64)
        .fetch_all(db)
        .await?;

        let points_history: Vec<PointEvent> = rows
            .into_iter()
            .map(|r| PointEvent {
                event_type: r.get("event_type"),
                points: r.get("points"),
                created_at: r.get("created_at"),
            })
            .collect();

        Ok(PointsJourneyOut { points_history })
    }

    /// 获取周订单日期
    pub async fn get_week_order_dates(
        db: &PgPool,
        group_id: i64,
    ) -> Result<WeekOrderDatesOut, CustomError> {
        let today = Local::now().date_naive();
        let week_ago = today - Duration::days(7);

        let rows = sqlx::query(
            "SELECT DISTINCT DATE(created_at) as order_date FROM orders \
             WHERE group_id = $1 AND created_at >= $2 \
             ORDER BY order_date",
        )
        .bind(group_id)
        .bind(week_ago)
        .fetch_all(db)
        .await?;

        let dates: Vec<NaiveDate> = rows.into_iter().map(|r| r.get("order_date")).collect();

        Ok(WeekOrderDatesOut { dates })
    }

    /// 获取日期菜品
    pub async fn get_date_foods(
        db: &PgPool,
        query: &DateQuery,
    ) -> Result<DateFoodsResponse, CustomError> {
        let rows = sqlx::query(
            "SELECT o.food_id, f.name, f.images, COUNT(*) as order_count \
             FROM orders o \
             JOIN foods f ON o.food_id = f.food_id \
             WHERE DATE(o.created_at) = $1 AND o.group_id = COALESCE($2, o.group_id) \
             GROUP BY o.food_id, f.name, f.images \
             ORDER BY order_count DESC",
        )
        .bind(query.date)
        .bind(query.group_id)
        .fetch_all(db)
        .await?;

        let foods: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "foodId": r.get::<i64, _>("food_id"),
                    "name": r.get::<String, _>("name"),
                    "images": r.get::<String, _>("images"),
                    "orderCount": r.get::<i64, _>("order_count"),
                })
            })
            .collect();

        Ok(DateFoodsResponse { foods })
    }
}
