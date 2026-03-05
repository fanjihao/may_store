use crate::{
    dashboard::models::{
        DateFoodOut, DateFoodsResponse, DateQuery, GroupActivityEventOut, GroupActivityQuery,
        JourneyOrderOut, OrderStatsOut, PointsJourneyOut, TodayOrderEntryOut, TodayOrdersResponse,
        TopFoodOrderOut, TopFoodRankingResponse, WeekDateInfo, WeekOrderDatesOut,
    },
    errors::CustomError,
    models::pagination::{decode_cursor, encode_cursor, CursorPage},
};
use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Deserialize, Serialize)]
pub struct ActivityCursor {
    pub occurred_at: DateTime<Utc>,
    pub ref_id: i64,
}

pub struct DashboardService;

impl DashboardService {
    pub async fn get_group_activities(
        db: &PgPool,
        group_id: i64,
        query: &GroupActivityQuery,
    ) -> Result<CursorPage<GroupActivityEventOut>, CustomError> {
        let limit = query.limit.unwrap_or(50).clamp(1, 200);

        let g_exists = sqlx::query("SELECT 1 FROM association_groups WHERE group_id=$1")
            .bind(group_id)
            .fetch_optional(db)
            .await?;
        if g_exists.is_none() {
            return Err(CustomError::BadRequest("关联组不存在".into()));
        }

        let cursor = query
            .cursor
            .as_deref()
            .and_then(decode_cursor::<ActivityCursor>);

        let sql = r#"
            SELECT * FROM (
                -- 订单创建
                SELECT
                    o.order_id AS ref_id,
                    o.user_id AS actor_user_id,
                    'ORDER_CREATED' AS event_type,
                    o.created_at AS occurred_at,
                    STRING_AGG(f.food_name,'+') AS ref_name,
                    NULL::int AS point_amount,
                    NULL::text AS point_tx_type,
                    NULL::int AS point_balance_after
                FROM orders o
                LEFT JOIN order_items oi ON o.order_id=oi.order_id
                LEFT JOIN foods f ON oi.food_id=f.food_id
                WHERE o.group_id=$1
                GROUP BY o.order_id, o.user_id, o.created_at

                UNION ALL
                -- 订单接单 / 完成 / 取消
                SELECT
                    osh.order_id AS ref_id,
                    osh.changed_by AS actor_user_id,
                    CASE
                        WHEN osh.to_status='ACCEPTED' THEN 'ORDER_ACCEPTED'
                        WHEN osh.to_status='FINISHED' THEN 'ORDER_FINISHED'
                        WHEN osh.to_status='CANCELLED' THEN 'ORDER_CANCELED'
                        ELSE 'ORDER_OTHER'
                    END AS event_type,
                    osh.changed_at AS occurred_at,
                    NULL::text AS ref_name,
                    NULL::int AS point_amount,
                    NULL::text AS point_tx_type,
                    NULL::int AS point_balance_after
                FROM order_status_history osh
                JOIN orders o ON osh.order_id=o.order_id
                WHERE o.group_id=$1
                  AND osh.to_status IN ('ACCEPTED','FINISHED','CANCELLED')

                UNION ALL
                -- 菜品申请 & 创建
                SELECT
                    f.food_id AS ref_id,
                    f.created_by AS actor_user_id,
                    CASE WHEN f.submit_role='ORDERING_APPLY' THEN 'FOOD_APPLIED' ELSE 'FOOD_CREATED' END AS event_type,
                    f.created_at AS occurred_at,
                    f.food_name AS ref_name,
                    NULL::int AS point_amount,
                    NULL::text AS point_tx_type,
                    NULL::int AS point_balance_after
                FROM foods f
                WHERE f.group_id=$1

                UNION ALL
                -- 菜品审核通过 / 拒绝
                SELECT
                    fal.food_id AS ref_id,
                    fal.acted_by AS actor_user_id,
                    CASE WHEN fal.action=2 THEN 'FOOD_APPROVED' ELSE 'FOOD_REJECTED' END AS event_type,
                    fal.created_at AS occurred_at,
                    f.food_name AS ref_name,
                    NULL::int AS point_amount,
                    NULL::text AS point_tx_type,
                    NULL::int AS point_balance_after
                FROM food_audit_logs fal
                JOIN foods f ON f.food_id=fal.food_id
                WHERE f.group_id=$1 AND fal.action IN (2, 3)

                UNION ALL
                -- 心愿创建
                SELECT
                    w.wish_id AS ref_id,
                    w.created_by AS actor_user_id,
                    'WISH_CREATED' AS event_type,
                    w.created_at AS occurred_at,
                    w.wish_name AS ref_name,
                    NULL::int AS point_amount,
                    NULL::text AS point_tx_type,
                    NULL::int AS point_balance_after
                FROM wishes w
                WHERE w.group_id=$1

                UNION ALL
                -- 心愿兑换
                SELECT
                    w.wish_id AS ref_id,
                    w.claimed_by AS actor_user_id,
                    'WISH_CLAIMED' AS event_type,
                    w.claimed_at AS occurred_at,
                    w.wish_name AS ref_name,
                    NULL::int AS point_amount,
                    NULL::text AS point_tx_type,
                    NULL::int AS point_balance_after
                FROM wishes w
                WHERE w.group_id=$1 AND w.claimed_at IS NOT NULL

                UNION ALL
                -- 积分流水（组内成员）
                SELECT
                    pt.id AS ref_id,
                    pt.user_id AS actor_user_id,
                    CASE
                        WHEN pt.type='ORDER_REWARD' THEN 'POINT_GAIN_ORDER'
                        WHEN pt.type='FINISH_REWARD' THEN 'POINT_GAIN_FINISH'
                        WHEN pt.type='WISH_COST' THEN 'POINT_COST_WISH'
                        WHEN pt.type='ORDER_RATING' THEN 'POINT_DELTA_RATING'
                        WHEN pt.type='ADMIN_ADJUST' THEN 'POINT_ADJUST_ADMIN'
                        WHEN pt.type='LOTTERY_REWARD' THEN 'POINT_GAIN_LOTTERY'
                        WHEN pt.type='SIGN_IN_REWARD' THEN 'POINT_GAIN_SIGN_IN'
                        ELSE 'POINT_OTHER'
                    END AS event_type,
                    pt.created_at AS occurred_at,
                    NULL::text AS ref_name,
                    pt.amount AS point_amount,
                    pt.type::text AS point_tx_type,
                    pt.balance_after AS point_balance_after
                FROM point_transactions pt
                JOIN association_group_members agm ON agm.user_id=pt.user_id AND agm.group_id=$1
                WHERE pt.type != 'SIGN_IN_REWARD'

                UNION ALL
                -- 签到记录（组内成员）
                SELECT
                    sr.sign_id AS ref_id,
                    sr.user_id AS actor_user_id,
                    'SIGN_IN' AS event_type,
                    sr.created_at AS occurred_at,
                    NULL::text AS ref_name,
                    sr.points_earned AS point_amount,
                    'SIGN_IN_REWARD'::text AS point_tx_type,
                    NULL::int AS point_balance_after
                FROM sign_records sr
                JOIN association_group_members agm ON agm.user_id=sr.user_id AND agm.group_id=$1
            ) all_events
            WHERE ($2::timestamptz IS NULL OR (occurred_at, ref_id) < ($2, $3))
            ORDER BY occurred_at DESC, ref_id DESC
            LIMIT $4
        "#;

        let rows = sqlx::query(sql)
            .bind(group_id)
            .bind(cursor.as_ref().map(|c| c.occurred_at))
            .bind(cursor.as_ref().map(|c| c.ref_id))
            .bind(limit + 1)
            .fetch_all(db)
            .await?;

        let has_more = rows.len() > limit as usize;
        let mut rows = rows;
        if has_more {
            rows.pop();
        }

        let next_cursor = if has_more {
            rows.last().map(|r| {
                encode_cursor(&ActivityCursor {
                    occurred_at: r.get("occurred_at"),
                    ref_id: r.get("ref_id"),
                })
            })
        } else {
            None
        };

        let items: Vec<GroupActivityEventOut> = rows
            .into_iter()
            .map(|r| GroupActivityEventOut {
                event_type: r.get::<String, _>("event_type"),
                actor_user_id: r.try_get("actor_user_id").ok(),
                ref_id: r.try_get("ref_id").ok(),
                ref_name: r.try_get("ref_name").ok(),
                occurred_at: r.get::<DateTime<Utc>, _>("occurred_at"),
                point_amount: r.try_get("point_amount").ok(),
                point_tx_type: r.try_get("point_tx_type").ok(),
                point_balance_after: r.try_get("point_balance_after").ok(),
            })
            .collect();

        Ok(CursorPage {
            items,
            next_cursor,
            has_more,
            total: None,
        })
    }

    pub async fn get_top_food_orders(db: &PgPool) -> Result<TopFoodRankingResponse, CustomError> {
        let rows = sqlx::query("SELECT oi.food_id, f.food_name, f.food_photo, COUNT(*)::bigint AS order_count FROM order_items oi JOIN orders o ON oi.order_id=o.order_id JOIN foods f ON oi.food_id=f.food_id GROUP BY oi.food_id, f.food_name, f.food_photo ORDER BY order_count DESC LIMIT 5")
            .fetch_all(db).await?;
        if rows.is_empty() {
            let random_rows = sqlx::query(
                "SELECT food_id, food_name, food_photo FROM foods ORDER BY random() LIMIT 5",
            )
            .fetch_all(db)
            .await?;
            if random_rows.is_empty() {
                return Ok(TopFoodRankingResponse {
                    list: vec![],
                    message: Some("暂无数据".into()),
                });
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
            return Ok(TopFoodRankingResponse {
                list,
                message: Some("无订单数据，随机推荐".into()),
            });
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
        Ok(TopFoodRankingResponse {
            list,
            message: None,
        })
    }

    pub async fn get_my_today_orders(
        db: &PgPool,
        user_id: i64,
    ) -> Result<TodayOrdersResponse, CustomError> {
        let rows = sqlx::query("SELECT o.order_id, o.status AS status, ARRAY_AGG(f.food_name) AS names, MIN(t.tag_name) AS tag_name FROM orders o JOIN order_items oi ON o.order_id=oi.order_id JOIN foods f ON oi.food_id=f.food_id LEFT JOIN tags t ON f.tag_id=t.tag_id WHERE o.user_id=$1 AND o.goal_time IS NOT NULL AND o.goal_time::date=CURRENT_DATE AND o.status IN ('PENDING','ACCEPTED','FINISHED') GROUP BY o.order_id, o.status")
            .bind(user_id)
            .fetch_all(db).await?;
        if rows.is_empty() {
            return Ok(TodayOrdersResponse {
                list: vec![],
                message: Some("暂无订单~".into()),
            });
        }
        let mut entries: Vec<TodayOrderEntryOut> = rows
            .into_iter()
            .map(|r| {
                let category: String = r
                    .get::<Option<String>, _>("tag_name")
                    .unwrap_or("其他".to_string());
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
        let order_rank = |c: &str| match c {
            "早上" => 1,
            "中午" => 2,
            "下午" => 3,
            "晚上" => 4,
            _ => 99,
        };
        entries.sort_by_key(|e| order_rank(&e.category));
        Ok(TodayOrdersResponse {
            list: entries,
            message: None,
        })
    }

    pub async fn get_my_order_stats(
        db: &PgPool,
        user_id: i64,
    ) -> Result<OrderStatsOut, CustomError> {
        let total_row = sqlx::query("SELECT COUNT(*)::bigint AS c FROM orders WHERE user_id=$1")
            .bind(user_id)
            .fetch_one(db)
            .await?;
        let finished_row = sqlx::query(
            "SELECT COUNT(*)::bigint AS c FROM orders WHERE user_id=$1 AND status='FINISHED'",
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;
        let rejected_row = sqlx::query(
            "SELECT COUNT(*)::bigint AS c FROM orders WHERE user_id=$1 AND status='REJECTED'",
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;
        Ok(OrderStatsOut {
            total_orders: total_row.get("c"),
            finished_orders: finished_row.get("c"),
            rejected_orders: rejected_row.get("c"),
        })
    }

    pub async fn get_points_journey(
        db: &PgPool,
        user_id: i64,
    ) -> Result<PointsJourneyOut, CustomError> {
        let order_rows = sqlx::query("SELECT o.order_id, o.status AS status, ARRAY_AGG(f.food_name) AS names FROM orders o JOIN order_items oi ON o.order_id=oi.order_id JOIN foods f ON oi.food_id=f.food_id WHERE o.user_id=$1 AND o.goal_time IS NOT NULL AND o.goal_time::date=CURRENT_DATE AND o.status IN ('PENDING','ACCEPTED') GROUP BY o.order_id, o.status")
            .bind(user_id).fetch_all(db).await?;
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
        let today_points_row = sqlx::query("SELECT COALESCE(SUM(amount),0)::bigint AS s FROM point_transactions WHERE user_id=$1 AND amount>0 AND created_at::date=CURRENT_DATE")
            .bind(user_id).fetch_one(db).await?;
        let total_gain_row = sqlx::query("SELECT COALESCE(SUM(amount),0)::bigint AS s FROM point_transactions WHERE user_id=$1 AND amount>0")
            .bind(user_id).fetch_one(db).await?;
        let total_cost_row = sqlx::query("SELECT COALESCE(SUM(-amount),0)::bigint AS s FROM point_transactions WHERE user_id=$1 AND amount<0")
            .bind(user_id).fetch_one(db).await?;
        let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1")
            .bind(user_id)
            .fetch_one(db)
            .await?;
        Ok(PointsJourneyOut {
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
        })
    }

    pub async fn get_week_order_dates(
        db: &PgPool,
        user_id: i64,
        query: &DateQuery,
    ) -> Result<WeekOrderDatesOut, CustomError> {
        let today = query.date.unwrap_or_else(|| Local::now().date_naive());
        let day_of_week = today.weekday().num_days_from_monday();
        let monday = today
            .checked_sub_days(chrono::Days::new(day_of_week as u64))
            .unwrap_or(today);

        use chrono::TimeZone;
        let monday_at_time = Local
            .with_ymd_and_hms(monday.year(), monday.month(), monday.day(), 0, 0, 0)
            .single()
            .ok_or_else(|| CustomError::InternalServerError("Invalid date/time conversion".into()))?
            .with_timezone(&Utc);
        let order_dates = sqlx::query!(
            r#"
            SELECT
                DATE(goal_time AT TIME ZONE 'Asia/Shanghai') AS order_date,
                COUNT(*)::int AS cnt
            FROM orders
            WHERE (user_id = $1 OR ($3::bigint IS NOT NULL AND group_id = $3))
                AND goal_time >= $2
                AND goal_time <  $2 + INTERVAL '7 days'
                AND status NOT IN ('CANCELLED', 'EXPIRED')
                GROUP BY DATE(goal_time AT TIME ZONE 'Asia/Shanghai');
            "#,
            user_id as i64,
            monday_at_time,
            query.group_id
        )
        .fetch_all(db)
        .await?;

        let order_date_set: std::collections::HashSet<NaiveDate> =
            order_dates.iter().filter_map(|r| r.order_date).collect();

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
                    day_of_week: i as i32 + 1,
                    has_order,
                    order_count: count,
                });
            }
        }

        Ok(WeekOrderDatesOut {
            week_dates,
            message: None,
        })
    }

    pub async fn get_date_foods(
        db: &PgPool,
        user_id: i64,
        query: &DateQuery,
    ) -> Result<DateFoodsResponse, CustomError> {
        let target_date = query.date.unwrap_or_else(|| Local::now().date_naive());

        let sql = r#"
            SELECT
                f.food_id,
                f.food_name,
                f.food_photo,
                f.ingredients,
                f.steps,
                t.tag_name,
                o.goal_time,
                o.status
            FROM orders o
            JOIN order_items oi ON o.order_id = oi.order_id
            JOIN foods f ON oi.food_id = f.food_id
            LEFT JOIN tags t ON f.tag_id = t.tag_id
            WHERE (o.user_id = $1 OR ($3::bigint IS NOT NULL AND o.group_id = $3))
              AND o.goal_time >= $2
              AND o.goal_time < $2 + INTERVAL '1 day'
              AND o.status NOT IN ('CANCELLED', 'EXPIRED')
            GROUP BY f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, t.tag_name, o.goal_time, o.status
            ORDER BY o.goal_time, f.food_id
            "#;

        let rows = sqlx::query(sql)
            .bind(user_id as i64)
            .bind(target_date)
            .bind(query.group_id)
            .fetch_all(db)
            .await?;

        struct IntermediateRow {
            food_id: i64,
            food_name: String,
            food_photo: Option<String>,
            steps: Option<String>,
            tag_name: Option<String>,
            goal_time: Option<chrono::DateTime<chrono::Utc>>,
            status: crate::models::orders::OrderStatusEnum,
            ingredient_ids: Vec<i64>,
        }

        let mut intermediates = Vec::with_capacity(rows.len());
        let mut all_ingredient_ids: HashSet<i64> = HashSet::new();

        for r in rows {
            let mut ids = Vec::new();
            if let Ok(ing_str) = r.try_get::<String, _>("ingredients") {
                if let Ok(parsed) = serde_json::from_str::<Vec<i64>>(&ing_str) {
                    ids = parsed;
                } else if let Ok(parsed_strs) = serde_json::from_str::<Vec<String>>(&ing_str) {
                    for s in parsed_strs {
                        if let Ok(id) = s.parse::<i64>() {
                            ids.push(id);
                        }
                    }
                } else {
                    if ing_str.contains(',') {
                        for part in ing_str.split(',') {
                            if let Ok(id) = part.trim().parse::<i64>() {
                                ids.push(id);
                            }
                        }
                    } else if let Ok(single_id) = ing_str.parse::<i64>() {
                        ids.push(single_id);
                    }
                }
            }
            all_ingredient_ids.extend(ids.iter().cloned());

            let status: crate::models::orders::OrderStatusEnum = r.get("status");

            intermediates.push(IntermediateRow {
                food_id: r.get("food_id"),
                food_name: r.get("food_name"),
                food_photo: r.try_get("food_photo").ok(),
                steps: r.try_get("steps").ok(),
                tag_name: r.try_get("tag_name").ok(),
                goal_time: r.try_get("goal_time").ok(),
                status,
                ingredient_ids: ids,
            });
        }

        let ingredient_map: HashMap<i64, crate::foods::models::ingredient::IngredientRecord> =
            if !all_ingredient_ids.is_empty() {
                let ids_vec: Vec<i64> = all_ingredient_ids.into_iter().collect();
                let ing_rows = sqlx::query_as::<_, crate::foods::models::ingredient::IngredientRecord>(
                "SELECT ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at FROM ingredients WHERE ingredient_id = ANY($1)"
            )
            .bind(&ids_vec)
            .fetch_all(db)
            .await?;

                ing_rows.into_iter().map(|r| (r.ingredient_id, r)).collect()
            } else {
                HashMap::new()
            };

        let foods_list: Vec<DateFoodOut> = intermediates
            .into_iter()
            .map(|row| {
                let mut ingredients_vec = Vec::new();
                for id in row.ingredient_ids {
                    if let Some(record) = ingredient_map.get(&id) {
                        ingredients_vec.push(record.clone());
                    }
                }

                DateFoodOut {
                    food_id: row.food_id,
                    food_name: row.food_name,
                    food_photo: row.food_photo,
                    ingredients: ingredients_vec,
                    steps: row.steps,
                    tag_name: row.tag_name,
                    reservation_time: row.goal_time,
                    status: row.status,
                }
            })
            .collect();

        let message = if foods_list.is_empty() {
            Some("当天暂无订单数据".into())
        } else {
            None
        };

        Ok(DateFoodsResponse {
            date: target_date,
            foods: foods_list,
            message,
        })
    }
}
