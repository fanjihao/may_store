// 应用服务层 - 心愿服务
// 包含心愿创建、认领、兑换、反馈等业务用例
// FSD.latest.md compliant - 7状态模型

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use sqlx::types::Json;

use crate::domain::wish::{
    WishCreateInput, WishFeedbackInput, WishQuoteInput, WishDeadlineInput, WishRejectInput,
    WishRecord, WishFeedbackRecord, WishStatus, WishUpdateInput, WishOut,
};
use crate::domain::event::{EventType, WishFulfilledPayload, WishNegotiatingPayload, WishAgreementConfirmedPayload, WishSelectedPayload};
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

    // ============== FSD v2 心愿协商与选择接口 ==============

    /// 协商报价 - 发起人或履约人报价或还价
    pub async fn quote_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishQuoteInput,
    ) -> Result<WishRecord, CustomError> {
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        // 检查状态：只有 DRAFT 或 NEGOTIATING 可以报价
        if !existing.status.is_negotiable() {
            return Err(CustomError::BadRequest("当前状态不允许报价".into()));
        }

        // 记录协商
        let action = if existing.status == WishStatus::Draft {
            // DRAFT 首次报价，进入 NEGOTIATING
            sqlx::query(
                "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, action, cost) VALUES ($1, $2, $3, 'QUOTE', $4)"
            )
            .bind(wish_id)
            .bind(existing.group_id)
            .bind(user_id)
            .bind(input.cost)
            .execute(db)
            .await?;

            // 更新状态为 NEGOTIATING，并记录 initial_cost
            sqlx::query_as::<_, WishRecord>(
                "UPDATE wishes SET status = 'NEGOTIATING', initial_cost = $2, updated_at = NOW() WHERE wish_id = $1 \
                 RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                 requester_id, fulfiller_id, initial_cost, final_cost"
            )
            .bind(wish_id)
            .bind(input.cost)
            .fetch_one(db)
            .await
        } else {
            // NEGOTIATING 阶段报价，记录还价
            sqlx::query(
                "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, action, cost) VALUES ($1, $2, $3, 'COUNTER', $4)"
            )
            .bind(wish_id)
            .bind(existing.group_id)
            .bind(user_id)
            .bind(input.cost)
            .execute(db)
            .await?;

            // 更新 final_cost（最终确认用）
            sqlx::query_as::<_, WishRecord>(
                "UPDATE wishes SET final_cost = $2, updated_at = NOW() WHERE wish_id = $1 \
                 RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                 requester_id, fulfiller_id, initial_cost, final_cost"
            )
            .bind(wish_id)
            .bind(input.cost)
            .fetch_one(db)
            .await
        };

        match action {
            Ok(rec) => Ok(rec),
            Err(e) => Err(CustomError::internal(e.to_string())),
        }
    }

    /// 协商履约期限
    pub async fn set_deadline(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishDeadlineInput,
    ) -> Result<WishRecord, CustomError> {
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        if !existing.status.is_negotiable() {
            return Err(CustomError::BadRequest("当前状态不允许设置期限".into()));
        }

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, action, deadline_hours) VALUES ($1, $2, $3, 'SET_DEADLINE', $4)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(input.deadline_hours)
        .execute(db)
        .await?;

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET fulfillment_deadline_hours = $2, updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .bind(input.deadline_hours)
        .fetch_one(db)
        .await?;

        Ok(rec)
    }

    /// 双方线上确认积分和期限，心愿进入 CREATED 状态（心愿池）
    pub async fn confirm_agreement(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        if existing.status != WishStatus::Negotiating {
            return Err(CustomError::BadRequest("当前状态不允许确认".into()));
        }

        // 确认后使用 final_cost 作为最终积分
        let final_cost = existing.final_cost.or(existing.initial_cost).unwrap_or(existing.wish_cost);

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, action, cost) VALUES ($1, $2, $3, 'ACCEPT', $4)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(final_cost)
        .execute(db)
        .await?;

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CREATED', wish_cost = $2, updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .bind(final_cost)
        .fetch_one(db)
        .await?;

        // 发布事件
        let _ = EventPublisher::publish(
            db,
            EventType::WishAgreementConfirmed,
            WishAgreementConfirmedPayload {
                wish_id,
                requester_id: existing.requester_id.unwrap_or(existing.created_by),
                fulfiller_id: existing.fulfiller_id.unwrap_or(0),
                group_id: existing.group_id,
                final_cost: existing.final_cost.unwrap_or(existing.wish_cost),
                fulfillment_deadline_hours: existing.fulfillment_deadline_hours.unwrap_or(72),
                trace_id: None,
            },
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        ).await;

        Ok(rec)
    }

    /// 拒绝或关闭心愿
    pub async fn reject_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishRejectInput,
    ) -> Result<WishRecord, CustomError> {
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        if existing.status.is_terminal() {
            return Err(CustomError::BadRequest("当前状态不允许关闭".into()));
        }

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, action, remark) VALUES ($1, $2, $3, 'CLOSE', $4)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(&input.reason)
        .execute(db)
        .await?;

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLOSED', closed_at = NOW(), updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        Ok(rec)
    }

    /// 选择心愿并冻结积分（发起人操作）
    pub async fn select_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, fulfillment_due_at, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        if existing.status != WishStatus::Created {
            return Err(CustomError::BadRequest("心愿当前不可选择".into()));
        }

        if existing.claimed_by.is_some() {
            return Err(CustomError::BadRequest("心愿已被选择".into()));
        }

        let points_cost = existing.final_cost.unwrap_or(existing.wish_cost);

        // 获取用户可用积分
        let user_row = sqlx::query_as::<_, (i32, Option<i64>)>(
            "SELECT love_point, group_id FROM users WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;

        let (current_points, user_group_id) = user_row;

        if current_points < points_cost {
            return Err(CustomError::BadRequest("爱心积分不足".into()));
        }

        let mut tx = db.begin().await?;

        // 冻结积分：可用减少，冻结增加
        let frozen_before = 0i32;
        let available_after = current_points - points_cost;
        let frozen_after = points_cost;

        sqlx::query(
            "UPDATE users SET love_point = $2 WHERE user_id = $1"
        )
        .bind(user_id)
        .bind(available_after)
        .execute(&mut *tx)
        .await?;

        // 写积分流水（冻结）
        sqlx::query(
            "INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, trace_id, created_at) \
             VALUES ($1, $2, 'FREEZE', $3, $4, $5, $6, $7, 'wish_select', $8, '', NOW())"
        )
        .bind(user_id)
        .bind(existing.group_id)
        .bind(points_cost)
        .bind(current_points)
        .bind(available_after)
        .bind(frozen_before)
        .bind(frozen_after)
        .bind(wish_id)
        .execute(&mut *tx)
        .await?;

        // 计算履约截止时间
        let deadline_hours = existing.fulfillment_deadline_hours.unwrap_or(72);
        let fulfillment_due_at = Utc::now() + Duration::hours(deadline_hours as i64);

        // 更新心愿状态为 CLAIMED
        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLAIMED', selected_by = $2, selected_at = NOW(), claimed_by = $2, fulfillment_due_at = $3, updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, fulfillment_due_at, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(fulfillment_due_at)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        // 发布事件
        let _ = EventPublisher::publish(
            db,
            EventType::WishSelected,
            WishSelectedPayload {
                wish_id,
                requester_id: user_id,
                fulfiller_id: existing.fulfiller_id.unwrap_or(0),
                group_id: existing.group_id,
                frozen_amount: points_cost as i32,
                fulfillment_due_at: fulfillment_due_at.to_rfc3339(),
                trace_id: None,
            },
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        ).await;

        Ok(rec)
    }
}
