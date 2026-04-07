use crate::errors::CustomError;
use crate::footprint::models::*;
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use chrono::Utc;
use sqlx::{FromRow, PgPool, Postgres, Row, Transaction};

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
        if amount == 0 {
            return Ok(0);
        }

        // 1. Update balance
        let new_balance = sqlx::query_scalar::<_, i32>(
            "UPDATE user_diamond SET diamond_balance = diamond_balance + $1, \
             total_get = total_get + CASE WHEN $1 > 0 THEN $1 ELSE 0 END, \
             total_consume = total_consume + CASE WHEN $1 < 0 THEN ABS($1) ELSE 0 END, \
             update_time = NOW() \
             WHERE user_id = $2 \
             RETURNING diamond_balance",
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

    pub async fn get_overview(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<FootprintOverview, CustomError> {
        // Together dining days: unique dates of completed orders in group
        let together_days = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT DATE(goal_time)) FROM orders \
             WHERE group_id = $1 AND status = 'CONFIRMED_FINISHED'",
        )
        .bind(group_id)
        .fetch_one(db)
        .await? as i32;

        // Total feedings: count of orders where user is receiver (guest_id) or group member role is receiving
        let total_feedings = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM orders WHERE group_id = $1 AND status = 'CONFIRMED_FINISHED'",
        )
        .bind(group_id)
        .fetch_one(db)
        .await? as i32;

        // Total records: official ones
        let total_records = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_record WHERE group_id = $1 AND is_draft = 0",
        )
        .bind(group_id)
        .fetch_one(db)
        .await? as i32;

        // Group capacity info
        let (footprint_capacity, footprint_count): (i32, i32) = sqlx::query_as(
            "SELECT footprint_capacity, footprint_count FROM association_groups WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_one(db)
        .await?;

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
            footprint_capacity,
            footprint_count,
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

    pub async fn list_record_groups(
        db: &PgPool,
        group_id: i64,
    ) -> Result<Vec<RecordGroup>, CustomError> {
        Self::ensure_default_groups(db, group_id).await?;
        let res = sqlx::query_as::<_, RecordGroup>(
            "SELECT * FROM record_group WHERE group_id = $1 AND status = 1 ORDER BY id ASC",
        )
        .bind(group_id)
        .fetch_all(db)
        .await?;
        Ok(res)
    }

    pub async fn get_record(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        record_id: i64,
    ) -> Result<RecordOut, CustomError> {
        let row = sqlx::query(
            "SELECT r.*, u.nick_name, u.avatar, \
             EXISTS(SELECT 1 FROM record_like l WHERE l.record_id = r.id AND l.user_id = $1) as is_liked \
             FROM user_record r LEFT JOIN users u ON u.user_id = r.user_id \
             WHERE r.id = $2 AND r.group_id = $3 AND r.is_draft = 0"
        )
        .bind(user_id)
        .bind(record_id)
        .bind(group_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("记录不存在".into()))?;

        let base = UserRecord::from_row(&row)?;
        Ok(RecordOut {
            base,
            user_nick_name: row.try_get("nick_name").ok(),
            user_avatar: row.try_get("avatar").ok(),
            is_liked: row.get("is_liked"),
        })
    }

    pub async fn list_records(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        record_group_id: Option<i64>,
        query: RecordQuery,
    ) -> Result<CursorPage<RecordOut>, CustomError> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100);

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT r.*, u.nick_name, u.avatar, \
             EXISTS(SELECT 1 FROM record_like l WHERE l.record_id = r.id AND l.user_id = ",
        );
        qb.push_bind(user_id);
        qb.push(") as is_liked FROM user_record r LEFT JOIN users u ON u.user_id = r.user_id WHERE r.group_id = ");
        qb.push_bind(group_id);

        if let Some(rg_id) = record_group_id {
            qb.push(" AND r.record_group_id = ");
            qb.push_bind(rg_id);
        }

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
        input: RecordCreateInput,
    ) -> Result<i64, CustomError> {
        let mut tx = db.begin().await?;

        // 1. Check global group capacity
        let cap_info: (i32, i32) = sqlx::query_as(
            "SELECT footprint_count, footprint_capacity FROM association_groups WHERE group_id = $1 FOR UPDATE"
        )
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;

        if cap_info.0 >= cap_info.1 {
            return Err(CustomError::BadRequest(
                "该组足迹记录容量已满，请解锁更多容量".into(),
            ));
        }

        // 2. Insert record
        let record_time = match &input.record_time {
            Some(s) if !s.is_empty() => chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map(|ndt| {
                        let offset = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
                        let dt_local = chrono::TimeZone::from_local_datetime(&offset, &ndt)
                            .single()
                            .unwrap_or_else(|| {
                                chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
                                    ndt, offset,
                                )
                            });
                        dt_local.with_timezone(&Utc)
                    })
                })
                .unwrap_or_else(|_| Utc::now()),
            _ => Utc::now(),
        };

        let record_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO user_record (group_id, record_group_id, user_id, title, images, content, address, record_time, is_draft) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0) \
             RETURNING id"
        )
        .bind(group_id)
        .bind(input.record_group_id)
        .bind(user_id)
        .bind(&input.title)
        .bind(input.images.join(","))
        .bind(&input.content)
        .bind(&input.address)
        .bind(record_time)
        .fetch_one(&mut *tx)
        .await?;

        // 3. Update global count
        sqlx::query("UPDATE association_groups SET footprint_count = footprint_count + 1 WHERE group_id = $1")
            .bind(group_id)
            .execute(&mut *tx)
            .await?;

        // 4. Update legacy count in record_group for backwards compatibility
        sqlx::query("UPDATE record_group SET current_count = current_count + 1 WHERE id = $1")
            .bind(input.record_group_id)
            .execute(&mut *tx)
            .await?;

        // 5. Award diamonds (max 20 per day from records)
        // Simplified limit check
        Self::award_diamonds(
            &mut tx,
            user_id,
            5,
            "record",
            Some(record_id),
            Some("发布足迹奖励".into()),
        )
        .await?;

        tx.commit().await?;
        Ok(record_id)
    }

    pub async fn update_record(
        db: &PgPool,
        user_id: i64,
        record_id: i64,
        input: RecordUpdateInput,
    ) -> Result<(), CustomError> {
        let mut tx = db.begin().await?;

        // Check ownership
        let record_user_id: i64 =
            sqlx::query_scalar("SELECT user_id FROM user_record WHERE id = $1")
                .bind(record_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| CustomError::BadRequest("记录不存在".into()))?;

        if record_user_id != user_id {
            return Err(CustomError::Forbidden("无权修改该记录".into()));
        }

        let mut qb =
            sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE user_record SET update_time = NOW()");

        if let Some(title) = &input.title {
            qb.push(", title = ");
            qb.push_bind(title);
        }
        if let Some(content) = &input.content {
            qb.push(", content = ");
            qb.push_bind(content);
        }
        if let Some(images) = &input.images {
            qb.push(", images = ");
            qb.push_bind(images.join(","));
        }
        if let Some(address) = &input.address {
            qb.push(", address = ");
            qb.push_bind(address);
        }
        if let Some(rg_id) = input.record_group_id {
            qb.push(", record_group_id = ");
            qb.push_bind(rg_id);
        }
        if let Some(s) = &input.record_time {
            let record_time = chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map(|ndt| {
                        let offset = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
                        let dt_local = chrono::TimeZone::from_local_datetime(&offset, &ndt)
                            .single()
                            .unwrap_or_else(|| {
                                chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
                                    ndt, offset,
                                )
                            });
                        dt_local.with_timezone(&Utc)
                    })
                })
                .map_err(|_| CustomError::BadRequest("时间格式错误".into()))?;
            qb.push(", record_time = ");
            qb.push_bind(record_time);
        }

        qb.push(" WHERE id = ");
        qb.push_bind(record_id);

        qb.build().execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_record(
        db: &PgPool,
        user_id: i64,
        record_id: i64,
    ) -> Result<(), CustomError> {
        let mut tx = db.begin().await?;

        // Check ownership and get group info
        let row =
            sqlx::query("SELECT user_id, group_id, record_group_id FROM user_record WHERE id = $1")
                .bind(record_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| CustomError::BadRequest("记录不存在".into()))?;

        let record_user_id: i64 = row.get("user_id");
        let group_id: i64 = row.get("group_id");
        let rg_id: i64 = row.get("record_group_id");

        if record_user_id != user_id {
            // Check if user is admin in this group
            let is_admin: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id = $1 AND user_id = $2 AND role_in_group = 'ADMIN')"
            )
            .bind(group_id)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;

            if !is_admin {
                return Err(CustomError::Forbidden("无权删除该记录".into()));
            }
        }

        // Delete record
        sqlx::query("DELETE FROM user_record WHERE id = $1")
            .bind(record_id)
            .execute(&mut *tx)
            .await?;

        // Update counts
        sqlx::query("UPDATE association_groups SET footprint_count = GREATEST(0, footprint_count - 1) WHERE group_id = $1")
            .bind(group_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "UPDATE record_group SET current_count = GREATEST(0, current_count - 1) WHERE id = $1",
        )
        .bind(rg_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn expand_capacity(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<i32, CustomError> {
        let mut tx = db.begin().await?;

        // 1. Get cost
        let cost: i32 = sqlx::query_scalar(
            "SELECT unlock_card_diamond_cost FROM group_point_configs WHERE group_id = $1",
        )
        .bind(group_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(100);

        // 2. Check and deduct diamonds
        let current_balance: i32 = sqlx::query_scalar(
            "SELECT diamond_balance FROM user_diamond WHERE user_id = $1 FOR UPDATE",
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

        if current_balance < cost {
            return Err(CustomError::BadRequest("钻石不足".into()));
        }

        Self::award_diamonds(
            &mut tx,
            user_id,
            -cost,
            "expand",
            Some(group_id),
            Some("解锁足迹容量".into()),
        )
        .await?;

        // 3. Update capacity
        let new_capacity: i32 = sqlx::query_scalar(
            "UPDATE association_groups SET footprint_capacity = footprint_capacity + 10 WHERE group_id = $1 RETURNING footprint_capacity"
        )
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(new_capacity)
    }

    pub async fn create_draft_from_order(db: &PgPool, order_id: i64) -> Result<(), CustomError> {
        let order =
            sqlx::query("SELECT user_id, group_id, created_at FROM orders WHERE order_id = $1")
                .bind(order_id)
                .fetch_one(db)
                .await?;

        let user_id: i64 = order.get("user_id");
        let group_id: Option<i64> = order.get("group_id");

        if let Some(gid) = group_id {
            // Find "干饭日常" group_id
            let rg_id: i64 = sqlx::query_scalar(
                "SELECT id FROM record_group WHERE group_id = $1 AND group_name = '干饭日常'",
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
