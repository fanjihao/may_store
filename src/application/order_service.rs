// 应用服务层 - 订单服务
// 包含订单创建、状态流转、评分等业务用例
// 使用 domain::order 和 infrastructure::event 模块

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use sqlx::{PgPool, Row};

use crate::domain::event::{types::*, EventType};
use crate::domain::order::{
    ExpGrantStatus, GroupInfoSimple, OrderCreateInput, OrderCursor, OrderItemOut, OrderItemRecord,
    OrderOutNew, OrderQuery, OrderRatingCreateInput, OrderRatingOut, OrderRecord, OrderStatistics,
    OrderStatus, OrderStatusHistoryOut, OrderStatusUpdateInput, TeamTodayOrdersQuery,
};
use crate::errors::CustomError;
use crate::infrastructure::event::publisher::EventPublisher;
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};

/// 订单应用服务
pub struct OrderService;

/// 从 global_configs 读整数配置
///
/// 跟 admin/routes.rs::read_int / footprints/routes.rs::read_global_int 行为一致:
/// 读不到时返回传入的 default。
async fn read_global_int(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &str,
    default: i32,
) -> i32 {
    sqlx::query_as::<_, (Option<serde_json::Value>,)>(
        "SELECT config_value FROM global_configs WHERE config_key = $1",
    )
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .ok()
    .flatten()
    .and_then(|(v,)| v)
    .and_then(|v| v.as_i64().map(|n| n as i32))
    .unwrap_or(default)
}

/// 当日 (UTC 日期) 该组某用户在 love_point_transactions 表里的累计变动 (正值 = 获得)
async fn sum_today_love_point_earned(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    group_id: i64,
) -> i64 {
    // 仅 EARN 类型, 当日 0 点到次日 0 点 (按 UTC 切日 - 后续可改成组时区)
    let row: (Option<i64>,) = sqlx::query_as(
        r#"SELECT COALESCE(SUM(amount), 0)::BIGINT
           FROM love_point_transactions
           WHERE user_id=$1 AND group_id=$2
             AND type='EARN'::love_point_tx_type_enum
             AND created_at >= date_trunc('day', NOW() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
             AND created_at <  date_trunc('day', NOW() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' + INTERVAL '1 day'"#,
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_one(&mut **tx)
    .await
    .unwrap_or((None,));
    row.0.unwrap_or(0)
}

/// 当日 (UTC 日期) 该组已发放的组经验 (EARN 类型)
async fn sum_today_group_exp_earned(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    group_id: i64,
) -> i64 {
    let row: (Option<i64>,) = sqlx::query_as(
        r#"SELECT COALESCE(SUM(amount), 0)::BIGINT
           FROM group_exp_transactions
           WHERE group_id=$1
             AND type='EARN'::group_exp_tx_type_enum
             AND created_at >= date_trunc('day', NOW() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
             AND created_at <  date_trunc('day', NOW() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' + INTERVAL '1 day'"#,
    )
    .bind(group_id)
    .fetch_one(&mut **tx)
    .await
    .unwrap_or((None,));
    row.0.unwrap_or(0)
}

impl OrderService {
    /// 创建订单
    pub async fn create_order(
        db: &PgPool,
        user_id: i64,
        input: &OrderCreateInput,
    ) -> Result<OrderOutNew, CustomError> {
        if input.items.is_empty() {
            return Err(CustomError::BadRequest("缺少菜品".into()));
        }
        let mut tx = db.begin().await?;

        // 校验用户组成员资格
        if let Some(gid) = input.group_id {
            let is_member = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
            )
            .bind(gid)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;

            if !is_member {
                let mut allowed = false;
                if let Some(code) = &input.invite_code {
                    let group_code: Option<String> = sqlx::query_scalar(
                        "SELECT invite_code FROM association_groups WHERE group_id=$1",
                    )
                    .bind(gid)
                    .fetch_optional(&mut *tx)
                    .await?
                    .flatten();

                    if let Some(gc) = group_code {
                        if gc == *code {
                            allowed = true;
                        }
                    }
                }

                if !allowed {
                    tx.rollback().await.ok();
                    return Err(CustomError::BadRequest("你不是该组成员且邀请码无效".into()));
                }
            }
        }

        let remark = input.remark.clone();
        let points_reward = input.points_reward.unwrap_or(0);
        let is_guest = input.is_guest.unwrap_or(input.group_id.is_none());

        let rec: OrderRecord = sqlx::query_as::<_, OrderRecord>(
            "INSERT INTO orders (user_id, guest_user_id, group_id, goal_time, remark, points_reward, is_guest) \
             VALUES ($1,$2,$3,$4,$5,$6,$7) \
             RETURNING order_id, user_id, guest_user_id AS guest_id, group_id, status, goal_time, remark, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at, is_guest"
        )
        .bind(user_id)
        .bind::<Option<i64>>(None)
        .bind(input.group_id)
        .bind(input.goal_time)
        .bind(remark)
        .bind(points_reward)
        .bind(is_guest)
        .fetch_one(&mut *tx)
        .await?;

        // 创建订单项
        for item in &input.items {
            let qty = item.quantity.unwrap_or(1).max(1);
            sqlx::query("INSERT INTO order_items (order_id, food_id, quantity) VALUES ($1,$2,$3)")
                .bind(rec.order_id)
                .bind(item.food_id)
                .bind(qty)
                .execute(&mut *tx)
                .await?;

            // 更新菜品统计
            sqlx::query(
                "INSERT INTO food_stats (food_id, total_order_count, last_order_time, updated_at) \
                 VALUES ($1, $2, NOW(), NOW()) \
                 ON CONFLICT (food_id) DO UPDATE \
                 SET total_order_count = food_stats.total_order_count + $2, \
                     last_order_time = NOW(), \
                     updated_at = NOW()",
            )
            .bind(item.food_id)
            .bind(qty)
            .execute(&mut *tx)
            .await?;
        }

        // 记录状态历史
        sqlx::query(
            "INSERT INTO order_status_history (order_id, from_status, to_status, changed_by) VALUES ($1, $2::order_status_enum, $3::order_status_enum, $4)"
        )
        .bind(rec.order_id)
        .bind::<Option<OrderStatus>>(None)
        .bind(OrderStatus::Created)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        // 获取订单项
        let items_out: Vec<OrderItemOut> = sqlx::query(
            "SELECT oi.id, oi.food_id, oi.quantity, oi.price, f.food_name, COALESCE(f.food_photo, f.images->0->>'url') AS food_photo \
             FROM order_items oi LEFT JOIN foods f ON f.food_id = oi.food_id WHERE oi.order_id=$1",
        )
        .bind(rec.order_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| OrderItemOut {
            id: r.get("id"),
            food_id: r.get("food_id"),
            food_name: r.try_get::<String, _>("food_name").ok(),
            food_photo: r.try_get::<Option<String>, _>("food_photo").ok().flatten(),
            quantity: r.get("quantity"),
            price: r.try_get("price").ok(),
        })
        .collect();

        // 获取状态历史
        let history_rows: Vec<OrderStatusHistoryOut> = sqlx::query(
            "SELECT h.from_status, h.to_status, u.nick_name, h.remark, h.changed_at \
             FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
             WHERE h.order_id=$1 ORDER BY h.changed_at",
        )
        .bind(rec.order_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(Self::map_history_row)
        .collect();

        tx.commit().await?;

        // 发布事件
        let _ = EventPublisher::publish(
            db,
            EventType::OrderCreated,
            OrderCreatedPayload {
                order_id: rec.order_id,
                user_id,
                group_id: rec.group_id,
                order_type: "NORMAL".to_string(),
                trace_id: None,
            },
            Some(user_id),
            rec.group_id,
            Some("order"),
            Some(rec.order_id),
        )
        .await;

        Ok(OrderOutNew {
            order_id: rec.order_id,
            user_id: rec.user_id,
            guest_id: rec.guest_id,
            group_id: rec.group_id,
            status: rec.status,
            goal_time: rec.goal_time,
            remark: rec.remark,
            points_reward: rec.points_reward,
            cancel_reason: rec.cancel_reason,
            reject_reason: rec.reject_reason,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
            last_status_change_at: rec.last_status_change_at,
            items: items_out,
            status_history: history_rows,
            is_guest: rec.is_guest,
            group_name: None,
            group_info: None,
            receiver_nick_name: None,
            receiver_avatar: None,
            daily_cap_warning: None,
        })
    }

    /// 获取订单列表
    pub async fn get_orders(
        db: &PgPool,
        token: &crate::middlewares::auth::UserToken,
        query: &OrderQuery,
    ) -> Result<CursorPage<OrderOutNew>, CustomError> {
        let user_id = token.user_id;
        let limit = query.limit.unwrap_or(50).clamp(1, 200);

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT o.order_id, o.user_id, o.guest_user_id AS guest_id, o.group_id, o.status, o.goal_time, o.remark, o.points_reward, o.cancel_reason, o.reject_reason, o.last_status_change_at, o.created_at, o.updated_at, \
            (o.group_id IS NOT NULL AND m.user_id IS NULL) AS is_guest, \
            g.group_name, \
            ug.nick_name AS db_guest_nick_name, ug.avatar AS db_guest_avatar, \
            uc.nick_name AS creator_nick_name, uc.avatar AS creator_avatar \
            FROM orders o \
            LEFT JOIN association_group_members m ON o.group_id = m.group_id AND o.user_id = m.user_id \
            LEFT JOIN association_groups g ON o.group_id = g.group_id \
            LEFT JOIN users ug ON o.guest_user_id = ug.user_id \
            LEFT JOIN users uc ON o.user_id = uc.user_id"
        );
        qb.push(" WHERE ");
        if let Some(gid) = query.group_id {
            let is_member = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
            )
            .bind(gid)
            .bind(user_id)
            .fetch_one(db)
            .await?;

            qb.push(" o.group_id = ");
            qb.push_bind(gid);
            if !is_member {
                qb.push(" AND (o.user_id = ");
                qb.push_bind(user_id);
                qb.push(" OR o.guest_user_id = ");
                qb.push_bind(user_id);
                qb.push(") ");
            }
        } else {
            qb.push(" (o.user_id = ");
            qb.push_bind(user_id);
            qb.push(" OR o.guest_user_id = ");
            qb.push_bind(user_id);
            qb.push(") ");
        }

        if let Some(st) = query.status {
            qb.push(" AND o.status = ");
            qb.push_bind(st);
        } else if query.expired_only.unwrap_or(false) {
            qb.push(" AND o.status IN ('TIMEOUT', 'CANCELLED', 'REJECTED', 'SYSTEM_CLOSED', 'BREEDER_CLOSED', 'CONFIRMED_UNFINISHED') ");
        }

        if let Some(cursor_str) = &query.cursor {
            if let Some(cursor) = decode_cursor::<OrderCursor>(cursor_str) {
                qb.push(" AND (o.created_at, o.order_id) < (");
                qb.push_bind(cursor.created_at);
                qb.push(", ");
                qb.push_bind(cursor.order_id);
                qb.push(")");
            }
        }

        qb.push(" ORDER BY o.created_at DESC, o.order_id DESC ");
        qb.push(" LIMIT ");
        qb.push_bind(limit + 1);

        let orders_rows = qb.build().fetch_all(db).await?;

        let has_more = orders_rows.len() > limit as usize;
        let mut rows = orders_rows;
        if has_more {
            rows.pop();
        }

        let next_cursor = if has_more {
            rows.last().map(|r| {
                encode_cursor(&OrderCursor {
                    created_at: r.get("created_at"),
                    order_id: r.get("order_id"),
                })
            })
        } else {
            None
        };

        let mut items: Vec<OrderOutNew> = Vec::new();
        for row in rows {
            let order = OrderRecord {
                order_id: row.get("order_id"),
                user_id: row.get("user_id"),
                guest_id: row.get("guest_id"),
                group_id: row.get("group_id"),
                status: row.get::<OrderStatus, _>("status"),
                goal_time: row.try_get("goal_time").ok(),
                remark: row.get("remark"),
                points_reward: row.get("points_reward"),
                cancel_reason: row.try_get("cancel_reason").ok(),
                reject_reason: row.try_get("reject_reason").ok(),
                last_status_change_at: row.try_get("last_status_change_at").ok(),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                is_guest: row.get("is_guest"),
                // FSD v2 new fields - use defaults
                type_: None,
                creator_role_snapshot: None,
                assignee_id: None,
                assignee_role_snapshot: None,
                guest_invite_id: None,
                guest_remark: None,
                guest_mark_tags: None,
                point_grant_status: None,
                exp_grant_status: None,
                risk_status: None,
                risk_detail: None,
                title: None,
                content: None,
                deadline: None,
                version: None,
                accepted_at: None,
                completed_at: None,
                confirmed_at: None,
            };
            let group_id: Option<i64> = row.get("group_id");
            let group_name: Option<String> = row.try_get("group_name").ok();
            let group_info = group_id.map(|gid| GroupInfoSimple {
                group_id: gid,
                group_name: group_name.clone(),
            });
            let db_guest_nick_name: Option<String> = row.try_get("db_guest_nick_name").ok();
            let db_guest_avatar: Option<String> = row.try_get("db_guest_avatar").ok();
            let creator_nick_name: Option<String> = row.try_get("creator_nick_name").ok();
            let creator_avatar: Option<String> = row.try_get("creator_avatar").ok();

            let order_items: Vec<OrderItemOut> = sqlx::query(
                "SELECT oi.id, oi.food_id, oi.quantity, oi.price, f.food_name, COALESCE(f.food_photo, f.images->0->>'url') AS food_photo \
                 FROM order_items oi LEFT JOIN foods f ON f.food_id = oi.food_id WHERE oi.order_id=$1",
            )
            .bind(order.order_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|r| OrderItemOut {
                id: r.get("id"),
                food_id: r.get("food_id"),
                food_name: r.try_get::<String, _>("food_name").ok(),
                food_photo: r.try_get::<Option<String>, _>("food_photo").ok().flatten(),
                quantity: r.get("quantity"),
                price: r.try_get("price").ok(),
            })
            .collect();
            let history_rows = sqlx::query(
                "SELECT h.from_status, h.to_status, u.nick_name, h.remark, h.changed_at \
                 FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
                 WHERE h.order_id=$1 ORDER BY h.changed_at DESC LIMIT 5",
            )
            .bind(order.order_id)
            .fetch_all(db)
            .await?;
            let history = history_rows
                .into_iter()
                .map(Self::map_history_row)
                .collect();
            let mut out = OrderOutNew {
                order_id: order.order_id,
                user_id: order.user_id,
                guest_id: order.guest_id,
                group_id: order.group_id,
                status: order.status,
                goal_time: order.goal_time,
                remark: order.remark,
                points_reward: order.points_reward,
                cancel_reason: order.cancel_reason,
                reject_reason: order.reject_reason,
                created_at: order.created_at,
                updated_at: order.updated_at,
                last_status_change_at: order.last_status_change_at,
                items: order_items,
                status_history: history,
                is_guest: order.is_guest,
                group_name: group_name.clone(),
                group_info,
                receiver_nick_name: db_guest_nick_name,
                receiver_avatar: db_guest_avatar,
                daily_cap_warning: None,
            };
            if out.guest_id.is_none() && out.is_guest {
                out.guest_id = Some(out.user_id);
                out.receiver_nick_name = creator_nick_name;
                out.receiver_avatar = creator_avatar;
            }
            items.push(out);
        }
        Ok(CursorPage {
            items,
            next_cursor,
            has_more,
            total: None,
        })
    }

    /// 获取团队今日订单
    pub async fn get_team_today_orders(
        db: &PgPool,
        user_id: i64,
        query: &TeamTodayOrdersQuery,
    ) -> Result<Vec<OrderOutNew>, CustomError> {
        let group_id = query.group_id;

        let is_member = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
        )
        .bind(group_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        if !is_member {
            return Err(CustomError::BadRequest("无权访问该组订单".into()));
        }

        let now = Local::now();
        let start_of_day = Local
            .with_ymd_and_hms(
                Datelike::year(&now),
                Datelike::month(&now),
                Datelike::day(&now),
                0,
                0,
                0,
            )
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let end_of_day = Local
            .with_ymd_and_hms(
                Datelike::year(&now),
                Datelike::month(&now),
                Datelike::day(&now),
                23,
                59,
                59,
            )
            .single()
            .unwrap()
            .with_timezone(&Utc);

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT o.order_id, o.user_id, o.guest_user_id AS guest_id, o.group_id, o.status, o.goal_time, o.remark, o.points_reward, o.cancel_reason, o.reject_reason, o.last_status_change_at, o.created_at, o.updated_at, \
            (o.group_id IS NOT NULL AND m.user_id IS NULL) AS is_guest, \
            g.group_name, \
            ug.nick_name AS db_guest_nick_name, ug.avatar AS db_guest_avatar, \
            uc.nick_name AS creator_nick_name, uc.avatar AS creator_avatar \
            FROM orders o \
            LEFT JOIN association_group_members m ON o.group_id = m.group_id AND o.user_id = m.user_id \
            LEFT JOIN association_groups g ON o.group_id = g.group_id \
            LEFT JOIN users ug ON o.guest_user_id = ug.user_id \
            LEFT JOIN users uc ON o.user_id = uc.user_id \
            WHERE o.group_id = "
        );
        qb.push_bind(group_id);
        qb.push(" AND o.goal_time >= ");
        qb.push_bind(start_of_day);
        qb.push(" AND o.goal_time <= ");
        qb.push_bind(end_of_day);
        qb.push(" AND o.status = ");
        qb.push_bind(OrderStatus::Created);
        qb.push(" ORDER BY o.goal_time ASC");

        let orders_rows = qb.build().fetch_all(db).await?;

        let mut items: Vec<OrderOutNew> = Vec::new();
        for row in orders_rows {
            let order = OrderRecord {
                order_id: row.get("order_id"),
                user_id: row.get("user_id"),
                guest_id: row.get("guest_id"),
                group_id: row.get("group_id"),
                status: row.get::<OrderStatus, _>("status"),
                goal_time: row.try_get("goal_time").ok(),
                remark: row.get("remark"),
                points_reward: row.get("points_reward"),
                cancel_reason: row.try_get("cancel_reason").ok(),
                reject_reason: row.try_get("reject_reason").ok(),
                last_status_change_at: row.try_get("last_status_change_at").ok(),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                is_guest: row.get("is_guest"),
                // FSD v2 new fields
                type_: None,
                creator_role_snapshot: None,
                assignee_id: None,
                assignee_role_snapshot: None,
                guest_invite_id: None,
                guest_remark: None,
                guest_mark_tags: None,
                point_grant_status: None,
                exp_grant_status: None,
                risk_status: None,
                risk_detail: None,
                title: None,
                content: None,
                deadline: None,
                version: None,
                accepted_at: None,
                completed_at: None,
                confirmed_at: None,
            };

            let group_id: Option<i64> = row.get("group_id");
            let group_name: Option<String> = row.try_get("group_name").ok();
            let group_info = group_id.map(|gid| GroupInfoSimple {
                group_id: gid,
                group_name: group_name.clone(),
            });
            let db_guest_nick_name: Option<String> = row.try_get("db_guest_nick_name").ok();
            let db_guest_avatar: Option<String> = row.try_get("db_guest_avatar").ok();
            let creator_nick_name: Option<String> = row.try_get("creator_nick_name").ok();
            let creator_avatar: Option<String> = row.try_get("creator_avatar").ok();

            let order_items: Vec<OrderItemOut> = sqlx::query(
                "SELECT oi.id, oi.food_id, oi.quantity, oi.price, f.food_name, COALESCE(f.food_photo, f.images->0->>'url') AS food_photo \
                 FROM order_items oi LEFT JOIN foods f ON f.food_id = oi.food_id WHERE oi.order_id=$1",
            )
            .bind(order.order_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|r| OrderItemOut {
                id: r.get("id"),
                food_id: r.get("food_id"),
                food_name: r.try_get::<String, _>("food_name").ok(),
                food_photo: r.try_get::<Option<String>, _>("food_photo").ok().flatten(),
                quantity: r.get("quantity"),
                price: r.try_get("price").ok(),
            })
            .collect();

            let history_rows = sqlx::query(
                "SELECT h.from_status, h.to_status, u.nick_name, h.remark, h.changed_at \
                 FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
                 WHERE h.order_id=$1 ORDER BY h.changed_at DESC LIMIT 5",
            )
            .bind(order.order_id)
            .fetch_all(db)
            .await?;
            let history = history_rows
                .into_iter()
                .map(Self::map_history_row)
                .collect();

            let mut out = OrderOutNew {
                order_id: order.order_id,
                user_id: order.user_id,
                guest_id: order.guest_id,
                group_id: order.group_id,
                status: order.status,
                goal_time: order.goal_time,
                remark: order.remark,
                points_reward: order.points_reward,
                cancel_reason: order.cancel_reason,
                reject_reason: order.reject_reason,
                created_at: order.created_at,
                updated_at: order.updated_at,
                last_status_change_at: order.last_status_change_at,
                items: order_items,
                status_history: history,
                is_guest: order.is_guest,
                group_name: group_name.clone(),
                group_info,
                receiver_nick_name: db_guest_nick_name,
                receiver_avatar: db_guest_avatar,
                daily_cap_warning: None,
            };
            if out.guest_id.is_none() && out.is_guest {
                out.guest_id = Some(out.user_id);
                out.receiver_nick_name = creator_nick_name;
                out.receiver_avatar = creator_avatar;
            }
            items.push(out);
        }

        Ok(items)
    }

    /// 获取订单详情
    pub async fn get_order_by_id(
        db: &PgPool,
        order_id: i64,
    ) -> Result<Option<OrderOutNew>, CustomError> {
        let row = sqlx::query(
            "SELECT o.order_id, o.user_id, o.guest_user_id AS guest_id, o.group_id, o.status, o.goal_time, o.remark, o.points_reward, o.cancel_reason, o.reject_reason, o.last_status_change_at, o.created_at, o.updated_at, \
            (o.group_id IS NOT NULL AND m.user_id IS NULL) AS is_guest, \
            g.group_name, \
            ur.nick_name AS db_receiver_nick_name, ur.avatar AS db_receiver_avatar, \
            uc.nick_name AS creator_nick_name, uc.avatar AS creator_avatar \
            FROM orders o \
            LEFT JOIN association_group_members m ON o.group_id = m.group_id AND o.user_id = m.user_id \
            LEFT JOIN association_groups g ON o.group_id = g.group_id \
            LEFT JOIN users ur ON o.guest_user_id = ur.user_id \
            LEFT JOIN users uc ON o.user_id = uc.user_id \
            WHERE o.order_id=$1"
        )
        .bind(order_id)
        .fetch_optional(db)
        .await?;

        let (
            order,
            group_name,
            db_receiver_nick_name,
            db_receiver_avatar,
            creator_nick_name,
            creator_avatar,
        ) = match row {
            Some(r) => (
                OrderRecord {
                    order_id: r.get("order_id"),
                    user_id: r.get("user_id"),
                    guest_id: r.get("guest_id"),
                    group_id: r.get("group_id"),
                    status: r.get::<OrderStatus, _>("status"),
                    goal_time: r.try_get("goal_time").ok(),
                    remark: r.get("remark"),
                    points_reward: r.get("points_reward"),
                    cancel_reason: r.try_get("cancel_reason").ok(),
                    reject_reason: r.try_get("reject_reason").ok(),
                    last_status_change_at: r.try_get("last_status_change_at").ok(),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    is_guest: r.get("is_guest"),
                    // FSD v2 fields
                    type_: r.try_get("type").ok().flatten(),
                    creator_role_snapshot: r.try_get("creator_role_snapshot").ok().flatten(),
                    assignee_id: r.try_get("assignee_id").ok().flatten(),
                    assignee_role_snapshot: r.try_get("assignee_role_snapshot").ok().flatten(),
                    // guest_id 已在 line 618 用 row.get("guest_id") 填充 (SQL 已 AS guest_id)
                    guest_invite_id: r.try_get("guest_invite_id").ok().flatten(),
                    guest_remark: r.try_get("guest_remark").ok().flatten(),
                    guest_mark_tags: r.try_get("guest_mark_tags").ok().flatten(),
                    point_grant_status: r.try_get("point_grant_status").ok().flatten(),
                    exp_grant_status: r.try_get("exp_grant_status").ok().flatten(),
                    risk_status: r.try_get("risk_status").ok().flatten(),
                    risk_detail: r.try_get("risk_detail").ok().flatten(),
                    title: r.try_get("title").ok().flatten(),
                    content: r.try_get("content").ok().flatten(),
                    deadline: r.try_get("deadline").ok().flatten(),
                    version: r.try_get("version").ok().flatten(),
                    accepted_at: r.try_get("accepted_at").ok().flatten(),
                    completed_at: r.try_get("completed_at").ok().flatten(),
                    confirmed_at: r.try_get("confirmed_at").ok().flatten(),
                },
                r.try_get::<String, _>("group_name").ok(),
                r.try_get("db_receiver_nick_name").ok(),
                r.try_get("db_receiver_avatar").ok(),
                r.try_get("creator_nick_name").ok(),
                r.try_get("creator_avatar").ok(),
            ),
            None => return Ok(None),
        };

        let item_rows = sqlx::query(
            "SELECT oi.id, oi.order_id, oi.food_id, oi.quantity, oi.price, oi.snapshot_json, oi.created_at, f.food_name, COALESCE(f.food_photo, f.images->0->>'url') AS food_photo \
             FROM order_items oi LEFT JOIN foods f ON f.food_id = oi.food_id WHERE oi.order_id=$1"
        )
        .bind(order.order_id)
        .fetch_all(db)
        .await?;
        let items = item_rows
            .into_iter()
            .map(|r| OrderItemOut {
                id: r.get("id"),
                food_id: r.get("food_id"),
                food_name: r.try_get::<String, _>("food_name").ok(),
                food_photo: r.try_get::<Option<String>, _>("food_photo").ok().flatten(),
                quantity: r.get("quantity"),
                price: r.try_get("price").ok(),
            })
            .collect();

        let hist_rows = sqlx::query(
            "SELECT h.from_status, h.to_status, u.nick_name, h.remark, h.changed_at \
                 FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
                 WHERE h.order_id=$1 ORDER BY h.changed_at",
        )
        .bind(order.order_id)
        .fetch_all(db)
        .await?;
        let history = hist_rows.into_iter().map(Self::map_history_row).collect();

        let mut out = OrderOutNew {
            order_id: order.order_id,
            user_id: order.user_id,
            guest_id: order.guest_id,
            group_id: order.group_id,
            status: order.status,
            goal_time: order.goal_time,
            remark: order.remark,
            points_reward: order.points_reward,
            cancel_reason: order.cancel_reason,
            reject_reason: order.reject_reason,
            created_at: order.created_at,
            updated_at: order.updated_at,
            last_status_change_at: order.last_status_change_at,
            items,
            status_history: history,
            is_guest: order.is_guest,
            group_name: group_name.clone(),
            group_info: order.group_id.map(|gid| GroupInfoSimple {
                group_id: gid,
                group_name: group_name.clone(),
            }),
            receiver_nick_name: db_receiver_nick_name,
            receiver_avatar: db_receiver_avatar,
            daily_cap_warning: None,
        };
        if out.guest_id.is_none() && out.is_guest {
            out.guest_id = Some(out.user_id);
            out.receiver_nick_name = creator_nick_name;
            out.receiver_avatar = creator_avatar;
        }
        Ok(Some(out))
    }

    /// 获取订单统计
    pub async fn get_order_statistics(
        db: &PgPool,
        group_id: i64,
    ) -> Result<OrderStatistics, CustomError> {
        let count = sqlx::query(
            "SELECT
                COALESCE(SUM(CASE WHEN status = 'PENDING_ACCEPT'::order_status_enum THEN 1 ELSE 0 END), 0) as pending_accept,
                COALESCE(SUM(CASE WHEN status = 'IN_PROGRESS'::order_status_enum THEN 1 ELSE 0 END), 0) as in_progress,
                COALESCE(SUM(CASE WHEN status = 'BREEDER_FINISHED'::order_status_enum THEN 1 ELSE 0 END), 0) as pending_confirm
            FROM
                orders
            WHERE
                group_id = $1
                AND status IN ('PENDING_ACCEPT'::order_status_enum, 'IN_PROGRESS'::order_status_enum, 'BREEDER_FINISHED'::order_status_enum)",
        )
        .bind(group_id)
        .fetch_one(db)
        .await?;

        let pending_accept: i64 = count.get("pending_accept");
        let in_progress: i64 = count.get("in_progress");
        let pending_confirm: i64 = count.get("pending_confirm");

        Ok(OrderStatistics {
            pending_accept: pending_accept as i32,
            in_progress: in_progress as i32,
            pending_confirm: pending_confirm as i32,
        })
    }

    /// 更新订单状态
    pub async fn update_order_status(
        db: &PgPool,
        user_id: i64,
        input: &OrderStatusUpdateInput,
    ) -> Result<OrderOutNew, CustomError> {
        let mut tx = db.begin().await?;

        let current: Option<OrderRecord> = sqlx::query_as::<_, OrderRecord>(
            "SELECT order_id, user_id, is_guest, guest_user_id AS guest_id, group_id, status, goal_time, remark, points_reward, cancel_reason, reject_reason, last_status_change_at, exp_grant_status, created_at, updated_at FROM orders WHERE order_id=$1 FOR UPDATE"
        )
        .bind(input.order_id)
        .fetch_optional(&mut *tx)
        .await?;
        let mut order = match current {
            Some(o) => o,
            None => {
                tx.rollback().await.ok();
                return Err(CustomError::BadRequest("订单不存在".into()));
            }
        };
        let from_status = order.status;

        if order.status == input.to_status {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("状态未变化".into()));
        }
        if !order.status.can_transition(input.to_status) {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("非法状态流转".into()));
        }

        match input.to_status {
            OrderStatus::Rejected => {
                sqlx::query(
                    "UPDATE orders SET status=$2, reject_reason=$3, last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1"
                )
                .bind(order.order_id)
                .bind(input.to_status)
                .bind(&input.remark)
                .execute(&mut *tx)
                .await?;
                order.reject_reason = input.remark.clone();
            }
            OrderStatus::Cancelled => {
                sqlx::query(
                    "UPDATE orders SET status=$2, cancel_reason=$3, last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1"
                )
                .bind(order.order_id)
                .bind(input.to_status)
                .bind(&input.remark)
                .execute(&mut *tx)
                .await?;
                order.cancel_reason = input.remark.clone();
            }
            _ => {
                sqlx::query(
                    "UPDATE orders SET status=$2, last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1"
                )
                .bind(order.order_id)
                .bind(input.to_status)
                .execute(&mut *tx)
                .await?;
            }
        }
        order.status = input.to_status;
        order.last_status_change_at = Some(Utc::now());

        // 订单完成时处理
        if matches!(input.to_status, OrderStatus::ConfirmedCompleted)
            && !matches!(from_status, OrderStatus::ConfirmedCompleted)
        {
            // 更新菜品完成统计
            let items: Vec<(i64, i32)> =
                sqlx::query_as("SELECT food_id, quantity FROM order_items WHERE order_id=$1")
                    .bind(order.order_id)
                    .fetch_all(&mut *tx)
                    .await?;
            for (fid, qty) in items {
                sqlx::query(
                    "INSERT INTO food_stats (food_id, total_order_count, completed_order_count, last_complete_time, updated_at) \
                     VALUES ($1, 0, $2, NOW(), NOW()) \
                     ON CONFLICT (food_id) DO UPDATE \
                     SET completed_order_count = food_stats.completed_order_count + $2, \
                         last_complete_time = NOW(), \
                         updated_at = NOW()"
                )
                .bind(fid)
                .bind(qty)
                .execute(&mut *tx)
                .await?;
            }
        }

        // 计算积分变动
        let pt_cfg = Self::get_group_point_config(&mut *tx, order.group_id).await;
        let points_delta = match input.to_status {
            OrderStatus::Cancelled => Some(pt_cfg.breeder_closed_points),
            OrderStatus::ConfirmedCompleted => Some(
                input
                    .points_reward
                    .unwrap_or(order.points_reward.max(pt_cfg.confirmed_finished_points)),
            ),
            OrderStatus::ConfirmedIncomplete => Some(pt_cfg.confirmed_unfinished_points),
            OrderStatus::Timeout => Some(pt_cfg.timeout_points),
            _ => None,
        };

        // 待 commit 后推送的 socket 事件 (2026-07-08)
        // 用 Option 包住是因为只有积分/经验真的变动时才需要推
        let mut push_love_point: Option<(i64, i32, String, Option<i64>)> = None;
        let mut push_group_exp: Option<(i64, i64, String)> = None;
        // 每日上限截断提示 —— 完成订单触达上限时返回给前端展示
        // 仅在 ConfirmedCompleted 时计算,其他状态不发奖励,谈不上截断
        let mut daily_cap_warning: Option<crate::domain::order::entities::DailyCapWarning> = None;
        // 当前订单状态对应的积分事件 reason
        let love_point_reason = match input.to_status {
            OrderStatus::Cancelled => "order_cancelled",
            OrderStatus::ConfirmedCompleted => "order_completed",
            OrderStatus::ConfirmedIncomplete => "order_incomplete",
            OrderStatus::Timeout => "order_timeout",
            _ => "order_other",
        };

        if let Some(delta) = points_delta {
            if delta != 0 {
                let group_id = match order.group_id {
                    Some(id) => id,
                    None => return Err(CustomError::BadRequest("订单缺少group_id".into())),
                };
                let receiver_user_id = match sqlx::query(
                    "SELECT user_id FROM association_group_members WHERE group_id=$1 AND role_in_group='RECEIVING'::group_member_role_enum LIMIT 1"
                )
                .bind(group_id)
                .fetch_optional(&mut *tx)
                .await
                {
                    Ok(Some(r)) => r.get::<i64, _>("user_id"),
                    _ => return Err(CustomError::BadRequest("未找到接单用户".into())),
                };

                // 2026-07-06 防刷单: 每日积分上限
                //   cap = base + level * level_step (基础值 + 等级增量)
                //   仅对正向奖励 (delta > 0) 生效; 扣分 (delta < 0) 不受上限影响
                //   超额部分直接截断, 不报错 (订单流程照常)
                let mut effective_delta = delta;
                let mut points_cap_warning: Option<crate::domain::order::entities::DailyCapItem> = None;
                if delta > 0 {
                    let base_cap = read_global_int(&mut tx, "dailyLovePointLimit", 100).await;
                    let level_step =
                        read_global_int(&mut tx, "dailyLovePointLimitLevelStep", 10).await;
                    let group_level: i32 = sqlx::query_as::<_, (i32,)>(
                        "SELECT level FROM association_groups WHERE group_id=$1",
                    )
                    .bind(group_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map(|(l,)| l)
                    .unwrap_or(1);
                    let actual_cap = base_cap + (group_level - 1).max(0) * level_step;
                    let already_earned =
                        sum_today_love_point_earned(&mut tx, receiver_user_id, group_id).await;
                    let room_left = (actual_cap as i64 - already_earned).max(0);
                    if room_left == 0 {
                        // 今天已经满额, 此次不发放
                        effective_delta = 0;
                    } else if (delta as i64) > room_left {
                        // 部分超出, 只发剩下的额度
                        effective_delta = room_left as i32;
                    }
                    // 记录截断提示 —— 仅当本次实际发的 < 原计划时记
                    if (effective_delta as i64) < (delta as i64) {
                        points_cap_warning = Some(crate::domain::order::entities::DailyCapItem {
                            granted: effective_delta as i64,
                            truncated: (delta as i64) - (effective_delta as i64),
                            daily_cap: actual_cap as i64,
                        });
                    }
                }

                if effective_delta != 0 {
                    if let Ok(user_row) =
                        sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
                            .bind(receiver_user_id)
                            .fetch_one(&mut *tx)
                            .await
                    {
                        let current_lp: i32 = user_row.get("love_point");
                        let balance_after = current_lp + effective_delta;
                        // love_point_tx_type_enum 合法值: EARN / FREEZE / UNFREEZE / DEDUCT / ADJUST
                        // 订单完成: 正值用 EARN, 负值用 DEDUCT
                        // 字符串到自定义 enum PG 不会隐式转换, 必须 ::love_point_tx_type_enum
                        let tx_type = if effective_delta >= 0 { "EARN" } else { "DEDUCT" };
                        sqlx::query(
                            r#"INSERT INTO love_point_transactions
                               (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id)
                               VALUES ($1, $2, $7::love_point_tx_type_enum, $3, $4, $5, 0, 0, 'ORDER', $6)"#
                        )
                            .bind(receiver_user_id)
                            .bind(group_id)
                            .bind(effective_delta)
                            .bind(current_lp as i64)
                            .bind(balance_after as i64)
                            .bind(order.order_id)
                            .bind(tx_type)
                            .execute(&mut *tx)
                            .await?;
                        sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
                            .bind(receiver_user_id)
                            .bind(balance_after)
                            .execute(&mut *tx)
                            .await?;
                        if effective_delta > 0 {
                            order.points_reward = effective_delta;
                        }

                        // 2026-07-08: 记录待推送的 love_point 变化 (commit 后调用)
                        push_love_point = Some((
                            receiver_user_id,
                            effective_delta,
                            love_point_reason.to_string(),
                            Some(order.order_id),
                        ));
                    }
                }
                // 把本次的截断信息(若有)累加到外层 warning
                if let Some(p) = points_cap_warning {
                    let entry = daily_cap_warning.get_or_insert_with(
                        || crate::domain::order::entities::DailyCapWarning {
                            points: None,
                            exp: None,
                        },
                    );
                    entry.points = Some(p);
                }
            }
        }

        // 组经验发放 (订单完成 → association_groups.exp + 流水 + 防重发)
        // - 触发: 仅 ConfirmedCompleted 且 orders.exp_grant_status = NONE
        // - 经验值: 读 global_configs.orderCompleteExp (默认 10), 没读到 → 0 (跳过发放)
        // - 防重发: 成功后把 exp_grant_status 设为 GRANTED
        // - 幂等: group_exp_transactions.idempotency_key 用 order:{order_id}:exp, 唯一索引兜底
        if matches!(input.to_status, OrderStatus::ConfirmedCompleted)
            && order.exp_grant_status != Some(ExpGrantStatus::Granted)
        {
            if let Some(group_id) = order.group_id {
                // 1) 读配置
                let exp_grant: i32 = read_global_int(&mut tx, "orderCompleteExp", 10).await;

                if exp_grant > 0 {
                    // 2) 锁住组行, 读当前 exp 和 level
                    let (current_exp, current_level): (i64, i32) = sqlx::query_as(
                        "SELECT exp, level FROM association_groups WHERE group_id=$1 FOR UPDATE"
                    )
                    .bind(group_id)
                    .fetch_one(&mut *tx)
                    .await?;

                    // 2026-07-06 防刷单: 每日经验上限
                    //   cap = base + (level - 1) * level_step (升级加量)
                    //   仅 EARN 类型; 已超额的订单直接截断, exp_grant_status 标 REJECTED_LIMIT
                    let base_exp_cap = read_global_int(&mut tx, "dailyGroupExpLimit", 200).await;
                    let exp_level_step =
                        read_global_int(&mut tx, "dailyGroupExpLimitLevelStep", 20).await;
                    let actual_exp_cap =
                        base_exp_cap + (current_level - 1).max(0) * exp_level_step;
                    let already_earned_today = sum_today_group_exp_earned(&mut tx, group_id).await;
                    let exp_room_left = (actual_exp_cap as i64 - already_earned_today).max(0);

                    let effective_exp_grant: i64 = if exp_room_left == 0 {
                        0
                    } else {
                        (exp_grant as i64).min(exp_room_left)
                    };

                    // 记录截断提示 —— 仅当实际发的 < 原计划时记
                    if effective_exp_grant < (exp_grant as i64) {
                        let entry = daily_cap_warning.get_or_insert_with(
                            || crate::domain::order::entities::DailyCapWarning {
                                points: None,
                                exp: None,
                            },
                        );
                        entry.exp = Some(crate::domain::order::entities::DailyCapItem {
                            granted: effective_exp_grant,
                            truncated: (exp_grant as i64) - effective_exp_grant,
                            daily_cap: actual_exp_cap as i64,
                        });
                    }

                    if effective_exp_grant > 0 {
                        let new_exp = current_exp + effective_exp_grant;

                        // 3) 重算等级 (从阶梯表找最大 level where required_exp <= new_exp)
                        let levels: Vec<(i32, i64)> = sqlx::query_as(
                            "SELECT level, required_exp FROM group_level_configs ORDER BY level ASC"
                        )
                        .fetch_all(&mut *tx)
                        .await?;
                        let new_level = levels
                            .iter()
                            .rev()
                            .find(|(_, req)| *req <= new_exp)
                            .map(|(lv, _)| *lv)
                            .unwrap_or(levels.first().map(|(lv, _)| *lv).unwrap_or(1));

                        // 4) 写流水 (幂等键防重)
                        let idem_key = format!("order:{}:exp", order.order_id);
                        sqlx::query(
                            r#"INSERT INTO group_exp_transactions
                               (group_id, type, amount, exp_before, exp_after, level_before, level_after, biz_type, biz_id, idempotency_key)
                               VALUES ($1, 'EARN'::group_exp_tx_type_enum, $2, $3, $4, $5, $6, 'ORDER', $7, $8)
                               ON CONFLICT (idempotency_key) DO NOTHING"#,
                        )
                        .bind(group_id)
                        .bind(effective_exp_grant)
                        .bind(current_exp)
                        .bind(new_exp)
                        .bind(current_level)
                        .bind(new_level)
                        .bind(order.order_id)
                        .bind(&idem_key)
                        .execute(&mut *tx)
                        .await?;

                        // 5) 更新组的 exp + level 缓存
                        sqlx::query(
                            "UPDATE association_groups SET exp=$2, level=$3, updated_at=NOW() WHERE group_id=$1"
                        )
                        .bind(group_id)
                        .bind(new_exp)
                        .bind(new_level)
                        .execute(&mut *tx)
                        .await?;
                    }

                    // 6) 标记订单 exp_grant_status
                    //   - 全部发完 (effective == exp_grant) → GRANTED
                    //   - 部分发了 / 完全没发 → REJECTED_LIMIT, 防重发但保留审计
                    let final_status = if effective_exp_grant as i32 == exp_grant {
                        "GRANTED"
                    } else {
                        "REJECTED_LIMIT"
                    };
                    sqlx::query(
                        "UPDATE orders SET exp_grant_status=$2::exp_grant_status_enum, updated_at=NOW() WHERE order_id=$1"
                    )
                    .bind(order.order_id)
                    .bind(final_status)
                    .execute(&mut *tx)
                    .await?;

                    // 2026-07-08: 记录待推送的组经验变化 (commit 后调用)
                    // 只推"实际发了经验"的情况, 没发的不打扰用户
                    // (新值 new_exp/new_level 已写入 group, push 函数会自己查最新值)
                    push_group_exp = Some((
                        group_id,
                        user_id,
                        "order_completed".to_string(),
                    ));
                }
            }
        }

        // 记录状态历史
        sqlx::query("INSERT INTO order_status_history (order_id, from_status, to_status, changed_by, remark) VALUES ($1, $2::order_status_enum, $3::order_status_enum, $4, $5)")
            .bind(order.order_id)
            .bind(from_status)
            .bind(input.to_status)
            .bind(user_id)
            .bind(&input.remark)
            .execute(&mut *tx)
            .await?;

        // 获取订单项
        let item_rows: Vec<OrderItemRecord> = sqlx::query_as::<_, OrderItemRecord>(
            "SELECT id, order_id, food_id, quantity, price, snapshot_json, created_at FROM order_items WHERE order_id=$1"
        )
        .bind(order.order_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut items_out: Vec<OrderItemOut> = Vec::new();
        for ir in item_rows {
            let food = sqlx::query("SELECT food_name, food_photo FROM foods WHERE food_id=$1")
                .bind(ir.food_id)
                .fetch_optional(&mut *tx)
                .await?;
            let (name_opt, photo_opt) = food
                .map(|r| {
                    (
                        r.get::<String, _>("food_name"),
                        r.get::<Option<String>, _>("food_photo"),
                    )
                })
                .map(|(n, p)| (Some(n), p))
                .unwrap_or((None, None));
            items_out.push(OrderItemOut {
                id: ir.id,
                food_id: ir.food_id,
                food_name: name_opt,
                food_photo: photo_opt,
                quantity: ir.quantity,
                price: ir.price,
            });
        }

        // 获取状态历史
        let history_rows: Vec<OrderStatusHistoryOut> = sqlx::query(
            "SELECT h.from_status, h.to_status, u.nick_name, h.remark, h.changed_at \
             FROM order_status_history h LEFT JOIN users u ON h.changed_by = u.user_id \
             WHERE h.order_id=$1 ORDER BY h.changed_at",
        )
        .bind(order.order_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(Self::map_history_row)
        .collect();

        tx.commit().await?;

        // === 2026-07-08: 推送 socket 事件给前端, 让积分/经验数字实时更新 ===
        // 注意: 必须在 commit 之后再推, 否则极端情况下"事务回滚但 socket 已推"会出 bug
        if let Some((user_id, delta, reason, order_id)) = push_love_point {
            crate::api::orders::broadcast::push_love_point_change_notice(
                db, user_id, delta, &reason, order_id,
            )
            .await;
        }
        if let Some((group_id, user_id, reason)) = push_group_exp {
            crate::api::orders::broadcast::push_group_exp_change_notice(
                db, group_id, user_id, &reason,
            )
            .await;
        }

        // 发布状态变更事件 (用于 /api/groups/{group_id}/activities 活动流)
        // 仅在 from != to 时发（防御性，正常情况前面已经拦截）
        if from_status != input.to_status {
            let event_type = match input.to_status {
                OrderStatus::Accepted => EventType::OrderAccepted,
                OrderStatus::ProductionCompleted => EventType::OrderCompleted,
                OrderStatus::ConfirmedCompleted => EventType::OrderConfirmedCompleted,
                OrderStatus::ConfirmedIncomplete => EventType::OrderConfirmedIncomplete,
                OrderStatus::Cancelled => EventType::OrderCancelled,
                OrderStatus::Rejected => EventType::OrderRejected,
                OrderStatus::Timeout => EventType::OrderTimeout,
                // Created 是独立入口 (create_order), 不在这发
                OrderStatus::Created => EventType::OrderCreated,
            };
            let _ = EventPublisher::publish(
                db,
                event_type,
                serde_json::json!({
                    "order_id": order.order_id,
                    "user_id": user_id,
                    "group_id": order.group_id,
                    "from_status": from_status,
                    "to_status": input.to_status,
                    "order_type": order.points_reward, // 留个口子, 真实 order_type 字段后续可以加
                }),
                Some(user_id),
                order.group_id,
                Some("order"),
                Some(order.order_id),
            );
        }

        Ok(OrderOutNew {
            order_id: order.order_id,
            user_id: order.user_id,
            guest_id: order.guest_id,
            group_id: order.group_id,
            status: order.status,
            goal_time: order.goal_time,
            remark: order.remark,
            points_reward: order.points_reward,
            cancel_reason: order.cancel_reason,
            reject_reason: order.reject_reason,
            created_at: order.created_at,
            updated_at: order.updated_at,
            last_status_change_at: order.last_status_change_at,
            items: items_out,
            status_history: history_rows,
            is_guest: order.is_guest,
            group_name: None,
            group_info: None,
            receiver_nick_name: None,
            receiver_avatar: None,
            daily_cap_warning,
        })
    }

    /// 删除订单
    pub async fn delete_order(db: &PgPool, user_id: i64, order_id: i64) -> Result<(), CustomError> {
        let row = sqlx::query("SELECT user_id FROM orders WHERE order_id=$1")
            .bind(order_id)
            .fetch_optional(db)
            .await?;

        match row {
            Some(r) => {
                let order_user_id = r.get::<i64, _>("user_id");
                if order_user_id != user_id {
                    return Err(CustomError::BadRequest("只能删除自己创建的订单".into()));
                }
            }
            None => {
                return Err(CustomError::BadRequest("订单不存在".into()));
            }
        }

        sqlx::query("DELETE FROM orders WHERE order_id=$1")
            .bind(order_id)
            .execute(db)
            .await?;

        Ok(())
    }

    /// 创建订单评价
    pub async fn create_order_rating(
        db: &PgPool,
        user_id: i64,
        order_id: i64,
        body: &OrderRatingCreateInput,
    ) -> Result<OrderRatingOut, CustomError> {
        if body.delta == 0 || body.delta.abs() > 5 {
            return Err(CustomError::BadRequest(
                "评分增减范围为 -5..5 且不能为0".into(),
            ));
        }
        let order_row = sqlx::query(
            "SELECT
                    o.order_id,
                    o.user_id,
                    o.status,
                    agm.user_id as target_user
                FROM
                    orders o
                LEFT JOIN
                    association_group_members agm
                ON
                    agm.group_id = o.group_id
                WHERE
                    o.order_id = $1 AND agm.role_in_group = 'RECEIVING'::group_member_role_enum FOR UPDATE",
        )
        .bind(order_id)
        .fetch_optional(db)
        .await?;
        let Some(or) = order_row else {
            return Err(CustomError::BadRequest("订单不存在".into()));
        };
        let status: OrderStatus = or.get("status");
        if status != OrderStatus::ConfirmedCompleted {
            return Err(CustomError::BadRequest("仅完成的订单可评分".into()));
        }
        let o_user_id: i64 = or.get("user_id");
        if o_user_id != user_id {
            return Err(CustomError::BadRequest("仅下单用户可评分".into()));
        }
        let receiver_id: i64 = or.get("target_user");

        let existing = sqlx::query("SELECT rating_id FROM order_ratings WHERE order_id=$1")
            .bind(order_id)
            .fetch_optional(db)
            .await?;
        if existing.is_some() {
            return Err(CustomError::BadRequest("该订单已评分".into()));
        }

        let mut tx = db.begin().await?;
        let target_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
            .bind(receiver_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(target_row) = target_row else {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("被评分用户不存在".into()));
        };
        let current_lp: i32 = target_row.get("love_point");
        let balance_after = current_lp + body.delta;
        sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
            .bind(receiver_id)
            .bind(balance_after)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO love_point_transactions (user_id, amount, type, biz_type, biz_id, available_after) VALUES ($1,$2,'ORDER_RATING','ORDER',$3,$4)"
        )
        .bind(receiver_id)
        .bind(body.delta)
        .bind(order_id)
        .bind(balance_after)
        .execute(&mut *tx)
        .await?;
        let rating_row = sqlx::query(
            "INSERT INTO order_ratings (order_id, rater_user_id, target_user_id, delta, remark) VALUES ($1,$2,$3,$4,$5) RETURNING rating_id, order_id, rater_user_id, target_user_id, delta, remark, created_at"
        )
        .bind(order_id)
        .bind(user_id)
        .bind(receiver_id)
        .bind(body.delta)
        .bind(&body.remark)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(OrderRatingOut {
            rating_id: rating_row.get("rating_id"),
            order_id: rating_row.get("order_id"),
            rater_user_id: rating_row.get("rater_user_id"),
            target_user_id: rating_row.get("target_user_id"),
            delta: rating_row.get("delta"),
            remark: rating_row.try_get("remark").ok(),
            created_at: rating_row.get("created_at"),
        })
    }

    /// 获取订单评价
    pub async fn get_order_rating(
        db: &PgPool,
        user_id: i64,
        order_id: i64,
    ) -> Result<Option<OrderRatingOut>, CustomError> {
        let order_row = sqlx::query("SELECT user_id, guest_user_id AS guest_id FROM orders WHERE order_id=$1")
            .bind(order_id)
            .fetch_optional(db)
            .await?;
        let Some(or) = order_row else {
            return Err(CustomError::BadRequest("订单不存在".into()));
        };
        let ouid: i64 = or.get("user_id");
        let rid_opt: Option<i64> = or.try_get("guest_id").ok(); // SQL 里有 AS 别名, 这里用别名读
        if ouid != user_id && rid_opt != Some(user_id) {
            return Err(CustomError::BadRequest("无权查看该订单评分".into()));
        }
        let rating_row = sqlx::query(
            "SELECT rating_id, order_id, rater_user_id, target_user_id, delta, remark, created_at FROM order_ratings WHERE order_id=$1"
        )
        .bind(order_id)
        .fetch_optional(db)
        .await?;
        let Some(rr) = rating_row else {
            return Ok(None);
        };
        Ok(Some(OrderRatingOut {
            rating_id: rr.get("rating_id"),
            order_id: rr.get("order_id"),
            rater_user_id: rr.get("rater_user_id"),
            target_user_id: rr.get("target_user_id"),
            delta: rr.get("delta"),
            remark: rr.try_get("remark").ok(),
            created_at: rr.get("created_at"),
        }))
    }

    // ========== 内部辅助方法 ==========

    fn map_history_row(row: sqlx::postgres::PgRow) -> OrderStatusHistoryOut {
        OrderStatusHistoryOut {
            from_status: row.get("from_status"),
            to_status: row.get("to_status"),
            changed_by: row.try_get("nick_name").ok().flatten(),
            remark: row.get::<Option<String>, _>("remark"),
            changed_at: row.get::<DateTime<Utc>, _>("changed_at"),
        }
    }

    async fn get_group_point_config(
        conn: &mut sqlx::PgConnection,
        group_id: Option<i64>,
    ) -> GroupPointConfigOut {
        let mut cfg = GroupPointConfigOut::default();

        // 1) 优先从 global_configs 读取系统级默认值
        //    multi-admin「系统配置」页改这 4 项即可生效，无需重启
        macro_rules! read_global_int {
            ($conn:expr, $key:literal, $default:expr) => {
                sqlx::query_as::<_, (Option<serde_json::Value>,)>(
                    "SELECT config_value FROM global_configs WHERE config_key = $1",
                )
                .bind($key)
                .fetch_optional($conn)
                .await
                .ok()
                .flatten()
                .and_then(|(v,)| v)
                .and_then(|v| v.as_i64().map(|n| n as i32))
                .unwrap_or($default)
            };
        }

        cfg.confirmed_finished_points = read_global_int!(
            &mut *conn,
            "confirmedFinishedPoints",
            cfg.confirmed_finished_points
        );
        cfg.confirmed_unfinished_points = read_global_int!(
            &mut *conn,
            "confirmedUnfinishedPoints",
            cfg.confirmed_unfinished_points
        );
        cfg.breeder_closed_points = read_global_int!(
            &mut *conn,
            "breederClosedPoints",
            cfg.breeder_closed_points
        );
        cfg.timeout_points = read_global_int!(
            &mut *conn,
            "timeoutPoints",
            cfg.timeout_points
        );

        // 2) per-group 覆盖 (FSD §11.23 group_configs 表) —— 已存在的 group 行会盖掉上面 4 个值
        //    group_configs.normal_order_love_point 替代旧 group_point_configs.confirmed_finished_points
        if let Some(gid) = group_id {
            if let Ok(Some(r)) = sqlx::query_as::<_, GroupPointConfigOut>(
                "SELECT group_id, \
                        breeder_closed_points, \
                        normal_order_love_point AS confirmed_finished_points, \
                        confirmed_unfinished_points, \
                        timeout_points, \
                        overdue_unfinished_points \
                 FROM group_configs WHERE group_id=$1"
            )
            .bind(gid)
            .fetch_optional(&mut *conn)
            .await {
                cfg = r;
            }
        }
        cfg
    }
}

/// 群组积分配置（用于订单服务）—— 从 group_configs (FSD §11.23) 读
#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GroupPointConfigOut {
    pub group_id: i64,
    pub breeder_closed_points: i32,
    pub confirmed_finished_points: i32,
    pub confirmed_unfinished_points: i32,
    pub timeout_points: i32,
    pub overdue_unfinished_points: i32,
}

impl Default for GroupPointConfigOut {
    fn default() -> Self {
        Self {
            group_id: 0,
            breeder_closed_points: 0,
            confirmed_finished_points: 0,
            confirmed_unfinished_points: 0,
            timeout_points: 0,
            overdue_unfinished_points: 0,
        }
    }
}
