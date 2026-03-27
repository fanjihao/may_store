use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use sqlx::{PgPool, Postgres, Transaction, FromRow, Row};
use crate::errors::CustomError;
use crate::footprint::models::*;
use chrono::{Utc};

pub struct FootprintService;

impl FootprintService {
    // --- Diamond Logic ---

    pub async fn get_user_diamond(db: &PgPool, user_id: i64) -> Result<UserDiamond, CustomError> {
        let res = sqlx::query_as::<_, UserDiamond>(
            "INSERT INTO user_diamond (user_id, diamond_balance, total_get, total_consume, create_time, update_time) \
             VALUES ($1, 0, 0, 0, NOW(), NOW()) \
             ON CONFLICT (user_id) DO UPDATE SET update_time = NOW() \
             RETURNING *"
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;
        Ok(res)
    }

    pub async fn award_diamonds(
        tx: &mut Transaction<'_, Postgres>,
        user_id: i64,
        amount: i32,
        scene: &str,
        relation_id: Option<i64>,
        remark: Option<String>,
    ) -> Result<i32, CustomError> {
        if amount == 0 { return Ok(0); }

        // 1. Update balance
        let new_balance = sqlx::query_scalar::<_, i32>(
            "UPDATE user_diamond SET diamond_balance = diamond_balance + $1, \
             total_get = total_get + CASE WHEN $1 > 0 THEN $1 ELSE 0 END, \
             total_consume = total_consume + CASE WHEN $1 < 0 THEN ABS($1) ELSE 0 END, \
             update_time = NOW() \
             WHERE user_id = $2 \
             RETURNING diamond_balance"
        )
        .bind(amount)
        .bind(user_id)
        .fetch_one(&mut **tx)
        .await?;

        // 2. Record flow
        // Generates a simple flow no without uuid crate
        let flow_no = format!("{}-{}-{}", scene, Utc::now().timestamp_millis(), user_id);
        sqlx::query(
            "INSERT INTO diamond_flow (flow_no, user_id, type, scene, diamond_num, balance_after, relation_id, remark) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(flow_no)
        .bind(user_id)
        .bind(if amount > 0 { 1i16 } else { 2i16 })
        .bind(scene)
        .bind(amount)
        .bind(new_balance)
        .bind(relation_id)
        .bind(remark)
        .execute(&mut **tx)
        .await?;

        Ok(new_balance)
    }

    // --- Overview Logic ---

    pub async fn get_overview(db: &PgPool, user_id: i64, group_id: i64) -> Result<FootprintOverview, CustomError> {
        // Together dining days: unique dates of completed orders in group
        let together_days = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT DATE(goal_time)) FROM orders \
             WHERE group_id = $1 AND status = 'CONFIRMED_FINISHED'"
        )
        .bind(group_id)
        .fetch_one(db)
        .await? as i32;

        // Total feedings: count of orders where user is receiver (guest_id) or group member role is receiving
        let total_feedings = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM orders WHERE group_id = $1 AND status = 'CONFIRMED_FINISHED'"
        )
        .bind(group_id)
        .fetch_one(db)
        .await? as i32;

        // Total records: official ones
        let total_records = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_record WHERE group_id = $1 AND is_draft = 0"
        )
        .bind(group_id)
        .fetch_one(db)
        .await? as i32;

        // Streak days (simplified: consecutive days with records in group)
        // In a real scenario, this needs a more complex query or pre-calculated field
        let streak_days = 0; // Placeholder for V1 MVP

        // Diamond balance
        let diamond = Self::get_user_diamond(db, user_id).await?;

        // Identity-specific text
        // Need to check if user is RECEIVING or ORDERING in this group
        let role: String = sqlx::query_scalar(
            "SELECT role_in_group::text FROM association_group_members WHERE group_id = $1 AND user_id = $2"
        )
        .bind(group_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .unwrap_or_else(|| "UNKNOWN".to_string());

        let feeding_text = if role == "RECEIVING" {
            format!("你已收到饲养员累计投喂 {} 次", total_feedings)
        } else {
            format!("你已为吃货累计投喂 {} 次", total_feedings)
        };

        Ok(FootprintOverview {
            together_days,
            total_feedings,
            streak_days,
            total_records,
            streak_progress: (streak_days % 7) as f32 / 7.0,
            feeding_text,
            diamond_balance: diamond.diamond_balance,
        })
    }

    // --- Record Management ---

    pub async fn ensure_default_groups(db: &PgPool, group_id: i64) -> Result<(), CustomError> {
        let defaults = vec!["干饭日常", "投喂日记", "专属纪念日"];
        for name in defaults {
            sqlx::query(
                "INSERT INTO record_group (group_id, group_name, group_type, max_capacity, current_count, status) \
                 VALUES ($1, $2, 1, 50, 0, 1) \
                 ON CONFLICT (group_id, group_name) DO NOTHING"
            )
            .bind(group_id)
            .bind(name)
            .execute(db)
            .await?;
        }
        Ok(())
    }

    pub async fn list_record_groups(db: &PgPool, group_id: i64) -> Result<Vec<RecordGroup>, CustomError> {
        Self::ensure_default_groups(db, group_id).await?;
        let res = sqlx::query_as::<_, RecordGroup>(
            "SELECT * FROM record_group WHERE group_id = $1 AND status = 1 ORDER BY id ASC"
        )
        .bind(group_id)
        .fetch_all(db)
        .await?;
        Ok(res)
    }

    pub async fn list_records(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        record_group_id: i64,
        query: RecordQuery,
    ) -> Result<CursorPage<RecordOut>, CustomError> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100);

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT r.*, u.nick_name, u.avatar, \
             EXISTS(SELECT 1 FROM record_like l WHERE l.record_id = r.id AND l.user_id = "
        );
        qb.push_bind(user_id);
        qb.push(") as is_liked FROM user_record r LEFT JOIN users u ON u.user_id = r.user_id WHERE r.group_id = ");
        qb.push_bind(group_id);
        qb.push(" AND r.record_group_id = ");
        qb.push_bind(record_group_id);
        qb.push(" AND r.is_draft = 0 ");

        if let Some(cursor_str) = &query.cursor {
            if let Some(cursor) = decode_cursor::<RecordCursor>(cursor_str) {
                qb.push(" AND (r.record_time, r.id) < (");
                qb.push_bind(cursor.record_time);
                qb.push(", ");
                qb.push_bind(cursor.id);
                qb.push(") ");
            }
        }

        qb.push(" ORDER BY r.record_time DESC, r.id DESC LIMIT ");
        qb.push_bind(limit + 1);

        let rows = qb.build().fetch_all(db).await?;
        let has_more = rows.len() > limit as usize;
        let mut rows = rows;
        if has_more {
            rows.pop();
        }

        let next_cursor = if has_more {
            rows.last().map(|r| {
                encode_cursor(&RecordCursor {
                    record_time: r.get("record_time"),
                    id: r.get("id"),
                })
            })
        } else {
            None
        };

        let mut items = Vec::new();
        for r in rows {
            let base = UserRecord::from_row(&r)?;
            items.push(RecordOut {
                base,
                user_nick_name: r.try_get("nick_name").ok(),
                user_avatar: r.try_get("avatar").ok(),
                is_liked: r.get("is_liked"),
            });
        }

        Ok(CursorPage {
            items,
            next_cursor,
            has_more,
            total: None,
        })
    }

    pub async fn submit_record(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        input: RecordCreateInput
    ) -> Result<i64, CustomError> {
        let mut tx = db.begin().await?;

        // 1. Check capacity
        let cap_info: (i32, i32) = sqlx::query_as(
            "SELECT current_count, max_capacity FROM record_group WHERE id = $1 AND group_id = $2 FOR UPDATE"
        )
        .bind(input.record_group_id)
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;

        if cap_info.0 >= cap_info.1 {
            return Err(CustomError::BadRequest("该分组记录容量已满，请扩容".into()));
        }

        // 2. Insert record
        let record_time = match &input.record_time {
            Some(s) if !s.is_empty() => {
                chrono::DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                            .map(|ndt| chrono::DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                    })
                    .unwrap_or_else(|_| Utc::now())
            }
            _ => Utc::now(),
        };

        let record_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO user_record (group_id, record_group_id, user_id, images, content, address, record_time, is_draft) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 0) \
             RETURNING id"
        )
        .bind(group_id)
        .bind(input.record_group_id)
        .bind(user_id)
        .bind(input.images.join(","))
        .bind(&input.content)
        .bind(&input.address)
        .bind(record_time)
        .fetch_one(&mut *tx)
        .await?;

        // 3. Update count
        sqlx::query("UPDATE record_group SET current_count = current_count + 1 WHERE id = $1")
            .bind(input.record_group_id)
            .execute(&mut *tx)
            .await?;

        // 4. Award diamonds (max 20 per day from records)
        // Simplified limit check
        Self::award_diamonds(&mut tx, user_id, 5, "record", Some(record_id), Some("发布足迹奖励".into())).await?;

        tx.commit().await?;
        Ok(record_id)
    }

    pub async fn create_draft_from_order(db: &PgPool, order_id: i64) -> Result<(), CustomError> {
        let order = sqlx::query(
            "SELECT user_id, group_id, created_at FROM orders WHERE order_id = $1"
        )
        .bind(order_id)
        .fetch_one(db)
        .await?;

        let user_id: i64 = order.get("user_id");
        let group_id: Option<i64> = order.get("group_id");

        if let Some(gid) = group_id {
            // Find "干饭日常" group_id
            let rg_id: i64 = sqlx::query_scalar(
                "SELECT id FROM record_group WHERE group_id = $1 AND group_name = '干饭日常'"
            )
            .bind(gid)
            .fetch_one(db)
            .await?;

            // Get items for content
            let items: Vec<String> = sqlx::query_scalar(
                "SELECT f.food_name FROM order_items oi JOIN foods f ON f.food_id = oi.food_id WHERE oi.order_id = $1"
            )
            .bind(order_id)
            .fetch_all(db)
            .await?;

            let content = format!("完成了订单，吃了：{}", items.join("、"));

            sqlx::query(
                "INSERT INTO user_record (group_id, record_group_id, user_id, order_id, images, content, is_draft) \
                 VALUES ($1, $2, $3, $4, '', $5, 1) \
                 ON CONFLICT (order_id) DO NOTHING"
            )
            .bind(gid)
            .bind(rg_id)
            .bind(user_id)
            .bind(order_id)
            .bind(content)
            .execute(db)
            .await?;
        }

        Ok(())
    }
}
