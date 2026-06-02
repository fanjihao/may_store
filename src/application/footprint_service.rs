// 应用服务层 - 足迹服务
// 包含足迹创建、发布、容量管理等业务用例

use chrono::Utc;
use sqlx::{PgPool, Row};
use crate::domain::footprint::{
    DraftConfirmInput, FootprintOverview, RecordCreateInput,
    RecordOut, RecordQuery, RecordUpdateInput, RecordGroup, FootprintRecord,
};
use crate::errors::CustomError;
use crate::models::pagination::CursorPage;

/// 足迹应用服务
#[allow(dead_code)]
pub struct FootprintService;

#[allow(dead_code)]
impl FootprintService {
    /// 创建足迹
    pub async fn create_record(
        db: &PgPool,
        user_id: i64,
        input: &RecordCreateInput,
    ) -> Result<i64, CustomError> {
        let record_time = if let Some(ref rt) = input.record_time {
            rt.clone()
        } else {
            Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
        };

        let images_str = input.images.join(",");

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO user_record (group_id, record_group_id, user_id, title, images, content, address, record_time) \
             SELECT $1, $2, $3, $4, $5, $6, $7, $8 WHERE EXISTS ( \
               SELECT 1 FROM record_group WHERE id = $2 AND group_id = $1 \
             ) RETURNING id"
        )
        .bind(input.record_group_id)
        .bind(input.record_group_id)
        .bind(user_id as i64)
        .bind(&input.title)
        .bind(&images_str)
        .bind(&input.content)
        .bind(&input.address)
        .bind(&record_time)
        .fetch_one(db)
        .await?;

        Ok(id)
    }

    /// 发布草稿
    pub async fn publish_draft(
        db: &PgPool,
        user_id: i64,
        input: &DraftConfirmInput,
    ) -> Result<i64, CustomError> {
        // 获取草稿
        let draft: Option<FootprintRecord> = sqlx::query_as(
            "SELECT id, group_id, record_group_id, user_id, order_id, title, images, content, address, record_time, like_count, comment_count, is_draft, create_time, update_time \
             FROM user_record WHERE id = $1 AND user_id = $2 AND is_draft = 1"
        )
        .bind(input.draft_id)
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?;

        let draft = draft.ok_or_else(|| CustomError::NotFound("草稿不存在".into()))?;

        // 更新内容
        let images_str = input.images.as_ref().map(|imgs| imgs.join(",")).unwrap_or(draft.images);

        sqlx::query(
            "UPDATE user_record SET images = $2, content = COALESCE($3, content), is_draft = 0 WHERE id = $1"
        )
        .bind(input.draft_id)
        .bind(&images_str)
        .bind(&input.content)
        .execute(db)
        .await?;

        Ok(input.draft_id)
    }

    /// 扩展容量
    pub async fn expand_capacity(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<i32, CustomError> {
        // 获取当前容量
        let (current_capacity, diamond_cost): (i32, i32) = sqlx::query_as(
            "SELECT COALESCE(default_footprint_capacity, 10), COALESCE(unlock_card_diamond_cost, 100) \
             FROM group_point_configs WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_optional(db)
        .await?
        .map(|r: (i32, i32)| r)
        .unwrap_or((10, 100));

        // 获取用户钻石
        let diamond_balance: i32 = sqlx::query("SELECT diamond FROM users WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        if diamond_balance < diamond_cost {
            return Err(CustomError::BadRequest("钻石不足".into()));
        }

        // 扣除钻石
        let new_balance = diamond_balance - diamond_cost;
        sqlx::query("UPDATE users SET diamond = $2 WHERE user_id = $1")
            .bind(user_id as i64)
            .bind(new_balance)
            .execute(db)
            .await?;

        // 扩展容量（每次+10）
        let new_capacity = current_capacity + 10;
        sqlx::query(
            "UPDATE group_point_configs SET default_footprint_capacity = $2 WHERE group_id = $1"
        )
        .bind(group_id)
        .bind(new_capacity)
        .execute(db)
        .await?;

        Ok(new_capacity)
    }

    /// 获取足迹概览
    pub async fn get_overview(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<FootprintOverview, CustomError> {
        // 获取总记录数
        let total_records: i32 = sqlx::query("SELECT COUNT(*) FROM user_record WHERE group_id = $1 AND is_draft = 0")
            .bind(group_id)
            .fetch_one(db)
            .await?
            .get(0);

        // 获取足迹容量
        let footprint_capacity: i32 = sqlx::query(
            "SELECT COALESCE(default_footprint_capacity, 50) FROM group_point_configs WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_one(db)
        .await?
        .get(0);

        // 获取当前记录数
        let footprint_count: i32 = sqlx::query("SELECT current_count FROM record_group WHERE group_id = $1 LIMIT 1")
            .bind(group_id)
            .fetch_optional(db)
            .await?
            .map(|r| r.get("current_count"))
            .unwrap_or(0);

        // 获取用户钻石
        let diamond_balance: i32 = sqlx::query("SELECT diamond FROM users WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        // 计算在一起天数和连续天数（需要关联组创建时间）
        let (together_days, streak_days) = sqlx::query_as::<_, (Option<i32>, Option<i32>)>(
            "SELECT CAST(EXTRACT(DAY FROM NOW() - create_time) AS INT), \
                    (SELECT COALESCE(MAX(consecutive_days), 0) FROM sign_records WHERE user_id = $1) \
             FROM association_groups WHERE group_id = $2"
        )
        .bind(user_id as i64)
        .bind(group_id)
        .fetch_optional(db)
        .await?
        .unwrap_or((Some(0), Some(0)));

        let streak_progress = if streak_days.unwrap_or(0) >= 7 { 1.0 } else {
            (streak_days.unwrap_or(0) as f32) / 7.0
        };

        let feeding_text = match streak_days.unwrap_or(0) {
            0 => "今日未签到".to_string(),
            1..=3 => "小试牛刀".to_string(),
            4..=6 => "渐入佳境".to_string(),
            7..=14 => "连续7天打卡".to_string(),
            15..=29 => "连续15天打卡".to_string(),
            30.. => "月度打卡达人".to_string(),
            _ => "未知".to_string(),
        };

        Ok(FootprintOverview {
            together_days: together_days.unwrap_or(0),
            total_feedings: total_records,
            streak_days: streak_days.unwrap_or(0),
            total_records,
            streak_progress,
            feeding_text,
            diamond_balance,
            footprint_capacity,
            footprint_count,
        })
    }

    /// 获取足迹分组列表
    pub async fn list_record_groups(
        db: &PgPool,
        group_id: i64,
    ) -> Result<Vec<RecordGroup>, CustomError> {
        let groups = sqlx::query_as::<_, RecordGroup>(
            "SELECT id, group_id, group_name, group_type, max_capacity, current_count, status, create_time, update_time \
             FROM record_group WHERE group_id = $1 AND status = 1 ORDER BY id"
        )
        .bind(group_id)
        .fetch_all(db)
        .await?;

        Ok(groups)
    }

    /// 获取足迹记录
    pub async fn get_record(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        record_id: i64,
    ) -> Result<RecordOut, CustomError> {
        let rec = sqlx::query_as::<_, FootprintRecord>(
            "SELECT id, group_id, record_group_id, user_id, order_id, title, images, content, address, record_time, like_count, comment_count, is_draft, create_time, update_time \
             FROM user_record WHERE id = $1 AND group_id = $2"
        )
        .bind(record_id)
        .bind(group_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("记录不存在".into()))?;

        // 获取用户信息
        let user_row = sqlx::query(
            "SELECT nick_name, avatar FROM users WHERE user_id = $1"
        )
        .bind(rec.user_id)
        .fetch_optional(db)
        .await?;

        let (nick_name, avatar) = if let Some(r) = user_row {
            (r.get("nick_name"), r.get("avatar"))
        } else {
            (None, None)
        };

        Ok(RecordOut {
            base: rec,
            user_nick_name: nick_name,
            user_avatar: avatar,
            is_liked: false,
        })
    }

    /// 获取足迹记录列表
    pub async fn list_records(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        record_group_id: Option<i64>,
        query: RecordQuery,
    ) -> Result<CursorPage<RecordOut>, CustomError> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100) as i64;

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT r.id, r.group_id, r.record_group_id, r.user_id, r.order_id, r.title, r.images, r.content, r.address, r.record_time, r.like_count, r.comment_count, r.is_draft, r.create_time, r.update_time \
             FROM user_record r WHERE r.group_id = $1 AND r.is_draft = 0"
        );

        qb.push(" AND r.group_id = ");
        qb.push_bind(group_id);

        if let Some(rgid) = record_group_id {
            qb.push(" AND r.record_group_id = ");
            qb.push_bind(rgid);
        }

        qb.push(" ORDER BY r.record_time DESC, r.id DESC LIMIT ");
        qb.push_bind(limit + 1);

        let rows = qb.build().fetch_all(db).await?;

        let has_more = rows.len() > limit as usize;

        let items: Vec<RecordOut> = rows.into_iter()
            .take(limit as usize)
            .map(|r| {
                let rec = FootprintRecord {
                    id: r.get("id"),
                    group_id: r.get("group_id"),
                    record_group_id: r.get("record_group_id"),
                    user_id: r.get("user_id"),
                    order_id: r.get("order_id"),
                    title: r.get("title"),
                    images: r.get("images"),
                    content: r.get("content"),
                    address: r.get("address"),
                    record_time: r.get("record_time"),
                    like_count: r.get("like_count"),
                    comment_count: r.get("comment_count"),
                    is_draft: r.get("is_draft"),
                    create_time: r.get("create_time"),
                    update_time: r.get("update_time"),
                };
                RecordOut {
                    base: rec,
                    user_nick_name: None,
                    user_avatar: None,
                    is_liked: false,
                }
            })
            .collect();

        Ok(CursorPage {
            items,
            next_cursor: None,
            has_more,
            total: None,
        })
    }

    /// 提交足迹记录
    pub async fn submit_record(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        input: RecordCreateInput,
    ) -> Result<i64, CustomError> {
        let record_time = input.record_time.unwrap_or_else(|| {
            Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
        });

        let images_str = input.images.join(",");

        // 检查容量
        let capacity: i32 = sqlx::query(
            "SELECT COALESCE(default_footprint_capacity, 50) FROM group_point_configs WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_one(db)
        .await?
        .get(0);

        let current_count: i32 = sqlx::query("SELECT COUNT(*) FROM user_record WHERE group_id = $1 AND is_draft = 0")
            .bind(group_id)
            .fetch_one(db)
            .await?
            .get(0);

        if current_count >= capacity {
            return Err(CustomError::BadRequest("足迹容量已满，请扩展容量".into()));
        }

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO user_record (group_id, record_group_id, user_id, title, images, content, address, record_time) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id"
        )
        .bind(group_id)
        .bind(input.record_group_id)
        .bind(user_id as i64)
        .bind(&input.title)
        .bind(&images_str)
        .bind(&input.content)
        .bind(&input.address)
        .bind(&record_time)
        .fetch_one(db)
        .await?;

        Ok(id)
    }

    /// 更新足迹记录
    pub async fn update_record(
        db: &PgPool,
        user_id: i64,
        record_id: i64,
        input: RecordUpdateInput,
    ) -> Result<(), CustomError> {
        // 检查权限
        let owner: Option<i64> = sqlx::query("SELECT user_id FROM user_record WHERE id = $1")
            .bind(record_id)
            .fetch_optional(db)
            .await?
            .map(|r| r.get("user_id"));

        if owner.map(|o| o != user_id as i64).unwrap_or(true) {
            return Err(CustomError::Forbidden("无权修改此记录".into()));
        }

        let images_str = input.images.as_ref().map(|imgs| imgs.join(","));

        sqlx::query(
            "UPDATE user_record SET \
             title = COALESCE($2, title), \
             images = COALESCE($3, images), \
             content = COALESCE($4, content), \
             address = COALESCE($5, address), \
             record_group_id = COALESCE($6, record_group_id) \
             WHERE id = $1"
        )
        .bind(record_id)
        .bind(&input.title)
        .bind(&images_str)
        .bind(&input.content)
        .bind(&input.address)
        .bind(&input.record_group_id)
        .execute(db)
        .await?;

        Ok(())
    }

    /// 删除足迹记录
    pub async fn delete_record(
        db: &PgPool,
        user_id: i64,
        record_id: i64,
    ) -> Result<(), CustomError> {
        // 检查权限
        let owner: Option<i64> = sqlx::query("SELECT user_id FROM user_record WHERE id = $1")
            .bind(record_id)
            .fetch_optional(db)
            .await?
            .map(|r| r.get("user_id"));

        if owner.map(|o| o != user_id as i64).unwrap_or(true) {
            return Err(CustomError::Forbidden("无权删除此记录".into()));
        }

        sqlx::query("DELETE FROM user_record WHERE id = $1")
            .bind(record_id)
            .execute(db)
            .await?;

        Ok(())
    }
}
