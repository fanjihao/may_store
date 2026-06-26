// 应用服务层 - 看板服务

use crate::domain::dashboard::*;
use crate::errors::CustomError;
use crate::models::pagination::{decode_cursor, encode_cursor};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

pub struct DashboardService;

/// 活动 cursor 结构: 按 created_at DESC + id DESC 翻页
#[derive(Debug, Serialize, Deserialize)]
struct ActivityCursor {
    created_at: DateTime<Utc>,
    id: i64,
}

impl DashboardService {
    /// 获取组活动 (cursor 翻页, 跟其他 list 接口风格一致)
    /// 入参: cursor=上一页最后一条的 "created_at,id" (base64), 第一页不传
    /// 出参: (events, next_cursor, has_more)
    pub async fn get_group_activities(
        db: &PgPool,
        group_id: i64,
        query: &GroupActivityQuery,
    ) -> Result<(Vec<GroupActivityEventOut>, Option<String>, bool), CustomError> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100);

        // 解码 cursor -> 上一页最后一条的 (created_at, id)
        // cursor 格式: base64({created_at, id})
        // 翻页条件: (created_at, id) < (cursor.created_at, cursor.id)
        let cursor = query
            .cursor
            .as_deref()
            .and_then(decode_cursor::<ActivityCursor>);

        // 拉 limit+1 行用于判断 has_more
        let rows = if let Some(c) = cursor {
            sqlx::query(
                "SELECT id, event_type, payload, user_id, created_at FROM event_log \
                 WHERE group_id = $1 AND (created_at, id) < ($2, $3) \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $4",
            )
            .bind(group_id)
            .bind(c.created_at)
            .bind(c.id)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        } else {
            sqlx::query(
                "SELECT id, event_type, payload, user_id, created_at FROM event_log \
                 WHERE group_id = $1 \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $2",
            )
            .bind(group_id)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        };

        let has_more = rows.len() as i64 > limit;
        let mut rows = rows;
        if has_more {
            rows.pop();
        }

        let last_row = rows.last();
        let next_cursor = if has_more {
            last_row.map(|r| {
                encode_cursor(&ActivityCursor {
                    created_at: r.get("created_at"),
                    id: r.get("id"),
                })
            })
        } else {
            None
        };

        let events: Vec<GroupActivityEventOut> = rows
            .into_iter()
            .map(|r| GroupActivityEventOut {
                event_type: r.get("event_type"),
                event_data: r.get("payload"),
                actor_user_id: r.try_get("user_id").ok().flatten(),
                created_at: r.get("created_at"),
            })
            .collect();

        Ok((events, next_cursor, has_more))
    }

    /// 获取热门菜品排名
    pub async fn get_top_food_orders(
        db: &PgPool,
        group_id: i64,
    ) -> Result<TopFoodRankingResponse, CustomError> {
        let rows = sqlx::query(
            "SELECT f.food_id, f.food_name, COUNT(*) as order_count \
             FROM orders o \
             LEFT JOIN order_items oi ON o.order_id = oi.order_id
             JOIN foods f ON oi.food_id = f.food_id \
             WHERE o.group_id = $1 AND o.status IN ('COMPLETED', 'CONFIRMED_COMPLETED') \
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
            "SELECT o.order_id, oi.food_id, o.status, o.created_at, f.food_name as food_name \
             FROM orders o \
             LEFT JOIN order_items oi ON o.order_id = oi.order_id
             JOIN foods f ON oi.food_id = f.food_id \
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
            sqlx::query("SELECT COUNT(*) FROM orders WHERE user_id = $1 AND status IN ('COMPLETED', 'CONFIRMED_COMPLETED')")
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
             FROM love_point_transactions WHERE user_id = $1 \
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
            "SELECT oi.food_id, f.food_name, f.images, COUNT(*) as order_count \
             FROM orders o \
             LEFT JOIN order_items oi ON o.order_id = oi.order_id
             JOIN foods f ON oi.food_id = f.food_id \
             WHERE DATE(o.created_at) = $1 AND o.group_id = COALESCE($2, o.group_id) \
             GROUP BY oi.food_id, f.food_name, f.images \
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
