// 应用服务层 - 心愿服务
// 包含心愿创建、认领、兑换、反馈等业务用例

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use sqlx::types::Json;

use crate::domain::wish::{
    WishCreateInput, WishFeedbackInput, WishOut, WishQuery, WishRecord,
    WishFeedbackRecord, WishFeedbackOut, WishStatus, WishUpdateInput,
};
use crate::domain::event::{EventType, WishFulfilledPayload};
use crate::errors::CustomError;
use crate::infrastructure::event::publisher::EventPublisher;

/// 心愿应用服务
pub struct WishService;

impl WishService {
    /// 创建心愿
    pub async fn create_wish(
        db: &PgPool,
        user_id: i64,
        input: &WishCreateInput,
    ) -> Result<WishRecord, CustomError> {
        let rec = sqlx::query_as::<_, WishRecord>(
            "INSERT INTO wishes (wish_name, wish_cost, created_by, group_id) \
             VALUES ($1, $2, $3, $4) \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(&input.wish_name)
        .bind(input.wish_cost)
        .bind(user_id as i64)
        .bind(input.group_id)
        .fetch_one(db)
        .await?;

        Ok(rec)
    }

    /// 获取心愿列表
    pub async fn list_wishes(
        db: &PgPool,
        group_id: i64,
        user_id: i64,
        limit: i64,
        cursor_condition: Option<(DateTime<Utc>, i64)>,
    ) -> Result<(Vec<WishRecord>, i64), CustomError> {
        let mut query = String::from(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE group_id = $1"
        );

        if let Some((cursor_time, cursor_id)) = cursor_condition {
            query.push_str(&format!(
                " AND (created_at, wish_id) < ('{}', {})",
                cursor_time.format("%Y-%m-%d %H:%M:%S%.f"),
                cursor_id
            ));
        }

        query.push_str(" ORDER BY created_at DESC, wish_id DESC");
        query.push_str(&format!(" LIMIT {}", limit + 1));

        let rows = sqlx::query_as::<_, WishRecord>(&query)
            .bind(group_id)
            .fetch_all(db)
            .await?;

        let total: i64 = sqlx::query("SELECT COUNT(*) FROM wishes WHERE group_id = $1")
            .bind(group_id)
            .fetch_one(db)
            .await?
            .get(0);

        Ok((rows, total))
    }

    /// 获取心愿详情
    pub async fn get_wish(
        db: &PgPool,
        wish_id: i64,
    ) -> Result<(WishRecord, Option<WishFeedbackRecord>), CustomError> {
        let rec = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT feedback_id, wish_id, user_id, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;

        Ok((rec, feedback))
    }

    /// 更新心愿
    pub async fn update_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishUpdateInput,
    ) -> Result<(WishRecord, Option<WishFeedbackRecord>), CustomError> {
        // 获取原心愿
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        // 检查权限
        if existing.created_by != user_id as i64 {
            return Err(CustomError::Forbidden("无权修改此心愿".into()));
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET wish_name = COALESCE($2, wish_name), wish_cost = COALESCE($3, wish_cost), status = COALESCE($4, status) \
             WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(wish_id)
        .bind(&input.wish_name)
        .bind(input.wish_cost)
        .bind(input.status)
        .fetch_one(db)
        .await?;

        let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT feedback_id, wish_id, user_id, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;

        Ok((rec, feedback))
    }

    /// 删除心愿
    pub async fn delete_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        // 获取原心愿
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        // 检查权限
        if existing.created_by != user_id as i64 {
            return Err(CustomError::Forbidden("无权删除此心愿".into()));
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLOSED' WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        Ok(rec)
    }

    /// 认领心愿（兑换）
    pub async fn redeem_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        // 获取心愿
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        // 检查状态
        if existing.status != WishStatus::Created {
            return Err(CustomError::BadRequest("心愿当前不可认领".into()));
        }

        // 检查是否已被人认领
        if existing.claimed_by.is_some() {
            return Err(CustomError::BadRequest("心愿已被认领".into()));
        }

        // 扣除积分
        let points_spent = existing.wish_cost;
        let user_points: i32 = sqlx::query("SELECT love_point FROM users WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        if user_points < points_spent {
            return Err(CustomError::BadRequest("积分不足".into()));
        }

        let new_points = user_points - points_spent;
        sqlx::query("UPDATE users SET love_point = $2 WHERE user_id = $1")
            .bind(user_id as i64)
            .bind(new_points)
            .execute(db)
            .await?;

        // 记录积分流水
        sqlx::query(
            "INSERT INTO point_flow (user_id, group_id, amount, balance, scene, relation_id) VALUES ($1, $2, $3, $4, 'wish', $5)"
        )
        .bind(user_id as i64)
        .bind(existing.group_id)
        .bind(-points_spent)
        .bind(new_points)
        .bind(wish_id)
        .execute(db)
        .await?;

        // 更新心愿状态
        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLAIMED', claimed_by = $2, claimed_at = NOW(), claim_cost = $3 WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(wish_id)
        .bind(user_id as i64)
        .bind(points_spent)
        .fetch_one(db)
        .await?;

        // 发布心愿兑换事件
        let payload = WishFulfilledPayload {
            wish_id,
            user_id,
            group_id: existing.group_id,
            points_spent,
        };
        let _ = EventPublisher::publish(
            db,
            EventType::WishFulfilled,
            payload,
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        ).await;

        Ok(rec)
    }

    /// 提交心愿反馈
    pub async fn submit_feedback(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishFeedbackInput,
    ) -> Result<(WishRecord, Option<WishFeedbackRecord>), CustomError> {
        // 获取心愿
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        // 检查权限（创建者或认领者可以提交反馈）
        if existing.created_by != user_id as i64 && existing.claimed_by.map(|c| c != user_id as i64).unwrap_or(true) {
            return Err(CustomError::Forbidden("无权提交此心愿反馈".into()));
        }

        // 检查心愿状态
        if existing.status != WishStatus::Claimed {
            return Err(CustomError::BadRequest("当前状态不允许提交反馈".into()));
        }

        // 插入或更新反馈
        let images_json = input.images.as_ref().map(|imgs| Json(imgs.clone()));

        sqlx::query(
            "INSERT INTO wish_feedbacks (wish_id, user_id, content, images) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (wish_id) DO UPDATE SET content = $3, images = $4"
        )
        .bind(wish_id)
        .bind(user_id as i64)
        .bind(&input.content)
        .bind(images_json)
        .execute(db)
        .await?;

        // 更新心愿状态为已完成
        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'FINISHED' WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        // 获取反馈记录
        let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT feedback_id, wish_id, user_id, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;

        Ok((rec, feedback))
    }
}
