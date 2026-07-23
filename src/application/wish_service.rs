// 应用服务层 - 心愿服务
// 包含心愿创建、认领、兑换、反馈等业务用例
// FSD.latest.md compliant - 7状态模型

use chrono::{DateTime, Duration, Utc};
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::domain::event::{
    EventType, WishAgreementConfirmedPayload, WishClosedPayload, WishExpiredPayload,
    WishFinishedPayload, WishNegotiatingPayload, WishSelectedPayload,
};
use crate::domain::wish::{
    validate_wish_cost, WishCreateInput, WishDeadlineInput, WishFeedbackInput, WishFeedbackRecord,
    WishNegotiationRecord, WishQuoteInput, WishRecord, WishRejectInput, WishStatus,
    WishUpdateInput,
};
use crate::errors::CustomError;
use crate::infrastructure::event::publisher::EventPublisher;

/// 心愿应用服务
pub struct WishService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WishClosureAction {
    Reject,
    Close,
}

impl WishClosureAction {
    fn negotiation_label(self) -> &'static str {
        match self {
            Self::Reject => "REJECT",
            Self::Close => "CLOSE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WishClosurePolicyError {
    NotParticipant,
    InvalidStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WishActorAccessError {
    GroupInactive,
    MemberInactive,
}

fn validate_active_wish_actor_snapshot(
    group_status: Option<&str>,
    member_status: Option<&str>,
) -> Result<(), WishActorAccessError> {
    if group_status != Some("ACTIVE") {
        return Err(WishActorAccessError::GroupInactive);
    }
    if member_status != Some("ACTIVE") {
        return Err(WishActorAccessError::MemberInactive);
    }
    Ok(())
}

fn validate_wish_closure_policy(
    status: WishStatus,
    action: WishClosureAction,
    actor_id: i64,
    requester_id: i64,
    fulfiller_id: i64,
) -> Result<&'static str, WishClosurePolicyError> {
    let role = if actor_id == requester_id {
        "REQUESTER"
    } else if actor_id == fulfiller_id {
        "FULFILLER"
    } else {
        return Err(WishClosurePolicyError::NotParticipant);
    };

    let status_allowed = match action {
        WishClosureAction::Reject => status == WishStatus::Negotiating,
        // 已领取后不能单方关闭；履约人应先走 release 回到心愿池。
        WishClosureAction::Close => {
            matches!(status, WishStatus::Negotiating | WishStatus::Created)
        }
    };
    if !status_allowed {
        return Err(WishClosurePolicyError::InvalidStatus);
    }

    Ok(role)
}

fn ensure_valid_wish_cost(cost: i32) -> Result<(), CustomError> {
    validate_wish_cost(cost).map_err(CustomError::invalid_parameter)
}

/// 在心愿写事务内锁定实际所属组和 actor 成员行，阻止校验后并发退组/停用。
async fn ensure_active_wish_actor_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    group_id: i64,
    actor_id: i64,
) -> Result<(), CustomError> {
    let snapshot: Option<(String, String)> = sqlx::query_as(
        "SELECT g.status::text, m.member_status::text \
         FROM association_groups g \
         JOIN association_group_members m \
           ON m.group_id = g.group_id AND m.user_id = $2 \
         WHERE g.group_id = $1 \
         FOR SHARE OF g, m",
    )
    .bind(group_id)
    .bind(actor_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (group_status, member_status) = match snapshot.as_ref() {
        Some((group_status, member_status)) => {
            (Some(group_status.as_str()), Some(member_status.as_str()))
        }
        None => (None, None),
    };

    match validate_active_wish_actor_snapshot(group_status, member_status) {
        Ok(()) => Ok(()),
        Err(WishActorAccessError::GroupInactive) => Err(CustomError::Forbidden(
            "心愿所属组已停用，不能继续操作".into(),
        )),
        Err(WishActorAccessError::MemberInactive) => Err(CustomError::Forbidden(
            "你已不是该心愿所属组的有效成员".into(),
        )),
    }
}

pub struct WishExpireResult {
    pub unfrozen_amount: i64,
    pub already_processed: bool,
}

impl WishService {
    async fn freeze_wish_points_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        wish_id: i64,
        group_id: i64,
        requester_id: i64,
        points_cost: i32,
    ) -> Result<(), CustomError> {
        let idempotency_key = format!("wish_pool_{wish_id}");
        let already_frozen: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM love_point_transactions WHERE idempotency_key = $1)",
        )
        .bind(&idempotency_key)
        .fetch_one(&mut **tx)
        .await?;
        if already_frozen {
            return Ok(());
        }

        let available_before: i32 =
            sqlx::query_scalar("SELECT love_point FROM users WHERE user_id = $1 FOR UPDATE")
                .bind(requester_id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| CustomError::NotFound("心愿创建人不存在".into()))?;
        if available_before < points_cost {
            return Err(CustomError::love_point_insufficient("爱心积分不足"));
        }
        let available_after = available_before - points_cost;

        let frozen_before: i64 = sqlx::query_scalar(
            "SELECT COALESCE(frozen_love_point, 0) FROM user_group_points \
             WHERE user_id = $1 AND group_id = $2 FOR UPDATE",
        )
        .bind(requester_id)
        .bind(group_id)
        .fetch_optional(&mut **tx)
        .await?
        .unwrap_or(0);
        let frozen_after = frozen_before
            .checked_add(i64::from(points_cost))
            .ok_or_else(|| CustomError::internal("冻结积分溢出"))?;

        sqlx::query("UPDATE users SET love_point = $2 WHERE user_id = $1")
            .bind(requester_id)
            .bind(available_after)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "INSERT INTO user_group_points \
                 (user_id, group_id, available_love_point, love_point, frozen_love_point, updated_at) \
             VALUES ($1, $2, $3, $3, $4, NOW()) \
             ON CONFLICT (user_id, group_id) DO UPDATE \
             SET available_love_point = $3, love_point = $3, frozen_love_point = $4, updated_at = NOW()",
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(i64::from(available_after))
        .bind(frozen_after)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO love_point_transactions
                   (user_id, group_id, type, amount, available_before, available_after,
                    frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at)
               VALUES
                   ($1, $2, 'FREEZE'::love_point_tx_type_enum, $3, $4, $5,
                    $6, $7, 'wish', $8, $9, NOW())"#,
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(points_cost)
        .bind(available_before)
        .bind(available_after)
        .bind(frozen_before)
        .bind(frozen_after)
        .bind(wish_id)
        .bind(idempotency_key)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn settle_wish_points_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        wish_id: i64,
        group_id: i64,
        requester_id: i64,
    ) -> Result<i64, CustomError> {
        let idempotency_key = format!("wish_finish_{wish_id}");
        let already_settled: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM love_point_transactions WHERE idempotency_key = $1)",
        )
        .bind(&idempotency_key)
        .fetch_one(&mut **tx)
        .await?;
        if already_settled {
            return Ok(0);
        }

        let frozen_amount: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(
                   SUM(CASE
                       WHEN type = 'FREEZE'::love_point_tx_type_enum THEN amount
                       WHEN type IN (
                           'UNFREEZE'::love_point_tx_type_enum,
                           'DEDUCT'::love_point_tx_type_enum
                       ) THEN -amount
                       ELSE 0
                   END),
                   0
               )::BIGINT
               FROM love_point_transactions
               WHERE user_id = $1 AND group_id = $2 AND biz_id = $3 AND biz_type = 'wish'"#,
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(wish_id)
        .fetch_one(&mut **tx)
        .await?;
        if frozen_amount <= 0 {
            return Ok(0);
        }

        let available: i32 =
            sqlx::query_scalar("SELECT love_point FROM users WHERE user_id = $1 FOR UPDATE")
                .bind(requester_id)
                .fetch_one(&mut **tx)
                .await?;
        let frozen_before: i64 = sqlx::query_scalar(
            "SELECT COALESCE(frozen_love_point, 0) FROM user_group_points \
             WHERE user_id = $1 AND group_id = $2 FOR UPDATE",
        )
        .bind(requester_id)
        .bind(group_id)
        .fetch_optional(&mut **tx)
        .await?
        .unwrap_or(frozen_amount);
        let frozen_after = frozen_before.saturating_sub(frozen_amount).max(0);

        sqlx::query(
            r#"INSERT INTO love_point_transactions
                   (user_id, group_id, type, amount, available_before, available_after,
                    frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at)
               VALUES
                   ($1, $2, 'DEDUCT'::love_point_tx_type_enum, $3, $4, $4,
                    $5, $6, 'wish', $7, $8, NOW())"#,
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(frozen_amount)
        .bind(available)
        .bind(frozen_before)
        .bind(frozen_after)
        .bind(wish_id)
        .bind(idempotency_key)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "UPDATE user_group_points SET frozen_love_point = $3, updated_at = NOW() \
             WHERE user_id = $1 AND group_id = $2",
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(frozen_after)
        .execute(&mut **tx)
        .await?;
        Ok(frozen_amount)
    }

    async fn load_feedbacks(
        db: &PgPool,
        wish_id: i64,
    ) -> Result<Vec<WishFeedbackRecord>, CustomError> {
        Ok(sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT feedback_id, wish_id, user_id, role_snapshot, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = $1 ORDER BY created_at ASC",
        )
        .bind(wish_id)
        .fetch_all(db)
        .await?)
    }

    /// 履约人已打卡、创建方超过 48 小时未处理时，下一次读取惰性自动完成。
    pub async fn auto_finish_overdue_wish(db: &PgPool, wish_id: i64) -> Result<bool, CustomError> {
        let mut tx = db.begin().await?;
        let snapshot: Option<(String, i64, i64, i64, i32, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT status::text, COALESCE(requester_id, created_by), \
                        COALESCE(fulfiller_id, claimed_by), group_id, \
                        COALESCE(final_cost, wish_cost), creator_checkin_due_at \
                 FROM wishes WHERE wish_id = $1 FOR UPDATE",
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((status, requester_id, fulfiller_id, group_id, cost, due_at)) = snapshot else {
            return Ok(false);
        };
        if status != "CLAIMED" || due_at.is_none_or(|due| due > Utc::now()) {
            tx.commit().await?;
            return Ok(false);
        }

        let fulfiller_checked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_feedbacks WHERE wish_id = $1 AND user_id = $2)",
        )
        .bind(wish_id)
        .bind(fulfiller_id)
        .fetch_one(&mut *tx)
        .await?;
        let requester_checked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_feedbacks WHERE wish_id = $1 AND user_id = $2)",
        )
        .bind(wish_id)
        .bind(requester_id)
        .fetch_one(&mut *tx)
        .await?;
        if !fulfiller_checked || requester_checked {
            tx.commit().await?;
            return Ok(false);
        }

        Self::settle_wish_points_in_tx(&mut tx, wish_id, group_id, requester_id).await?;
        let updated = sqlx::query(
            "UPDATE wishes SET status = 'FINISHED'::wish_status_enum, finished_at = NOW(), \
                    auto_completed_at = NOW(), creator_checkin_due_at = NULL, updated_at = NOW() \
             WHERE wish_id = $1 AND status = 'CLAIMED'::wish_status_enum",
        )
        .bind(wish_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if updated.rows_affected() != 1 {
            return Ok(false);
        }

        let _ = EventPublisher::publish(
            db,
            EventType::WishFinished,
            WishFinishedPayload {
                wish_id,
                requester_id,
                fulfiller_id,
                group_id,
                deducted_amount: cost,
                trace_id: None,
            },
            None,
            Some(group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;
        Ok(true)
    }

    /// 履约截止后仍没有履约方打卡时，下一次读取惰性过期并退回冻结积分。
    pub async fn auto_expire_overdue_wish(db: &PgPool, wish_id: i64) -> Result<bool, CustomError> {
        let mut tx = db.begin().await?;
        let snapshot: Option<(String, i64, i64, i64, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT status::text, COALESCE(requester_id, created_by), \
                    COALESCE(fulfiller_id, claimed_by), group_id, fulfillment_due_at \
             FROM wishes WHERE wish_id = $1 FOR UPDATE",
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((status, requester_id, fulfiller_id, group_id, due_at)) = snapshot else {
            return Ok(false);
        };
        if status != "CLAIMED" || due_at.is_none_or(|due| due > Utc::now()) {
            tx.commit().await?;
            return Ok(false);
        }

        let fulfiller_checked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_feedbacks WHERE wish_id = $1 AND user_id = $2)",
        )
        .bind(wish_id)
        .bind(fulfiller_id)
        .fetch_one(&mut *tx)
        .await?;
        if fulfiller_checked {
            tx.commit().await?;
            return Ok(false);
        }

        let updated = sqlx::query(
            "UPDATE wishes SET status = 'EXPIRED'::wish_status_enum, expired_at = NOW(), \
                    updated_at = NOW() \
             WHERE wish_id = $1 AND status = 'CLAIMED'::wish_status_enum",
        )
        .bind(wish_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            tx.commit().await?;
            return Ok(false);
        }
        let idempotency_key = format!("wish_expire_{wish_id}");
        let unfrozen_amount = Self::unfreeze_wish_points_in_tx(
            &mut tx,
            wish_id,
            group_id,
            requester_id,
            &idempotency_key,
        )
        .await?;
        tx.commit().await?;

        let _ = EventPublisher::publish(
            db,
            EventType::WishExpired,
            WishExpiredPayload {
                wish_id,
                requester_id,
                fulfiller_id,
                group_id,
                unfrozen_amount: i32::try_from(unfrozen_amount)
                    .map_err(|_| CustomError::internal("心愿解冻积分超出支持范围"))?,
                trace_id: None,
            },
            None,
            Some(group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;
        Ok(true)
    }

    pub async fn auto_process_overdue_group_wishes(
        db: &PgPool,
        group_id: i64,
    ) -> Result<(), CustomError> {
        let wish_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT wish_id FROM wishes \
             WHERE group_id = $1 AND status = 'CLAIMED'::wish_status_enum \
               AND ((creator_checkin_due_at IS NOT NULL AND creator_checkin_due_at <= NOW()) \
                    OR (fulfillment_due_at IS NOT NULL AND fulfillment_due_at <= NOW()))",
        )
        .bind(group_id)
        .fetch_all(db)
        .await?;
        for wish_id in wish_ids {
            Self::auto_finish_overdue_wish(db, wish_id).await?;
            Self::auto_expire_overdue_wish(db, wish_id).await?;
        }
        Ok(())
    }

    /// 创建心愿
    pub async fn create_wish(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        input: &WishCreateInput,
    ) -> Result<WishRecord, CustomError> {
        ensure_valid_wish_cost(input.wish_cost)?;

        // FSD v2 心愿商城: 创建即进入 NEGOTIATING，跳过 DRAFT，省去"邀请协商"步骤
        //
        // 创建时自动把组内另一成员设为 fulfiller_id(1v1 模型),
        // 创建人自己 = requester_id。这样后续 confirm_agreement 的
        // 「双方都确认」校验、select_wish 的「请求人/履约人」语义、
        // 以及 list_group_wishes JOIN users u1/u2 都不会因 NULL 而漏数据。

        // 1. 找组内另一名 ACTIVE 成员 → 自动设为 fulfiller
        let other_id: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM association_group_members \
             WHERE group_id = $1 AND user_id != $2 AND member_status = 'ACTIVE'::group_member_status_enum \
             LIMIT 1",
        )
        .bind(group_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
        let fulfiller_id = other_id
            .ok_or_else(|| CustomError::BadRequest("组内需要至少 2 名成员才能创建心愿".into()))?;

        // 2. 同组同时只能有 1 条 DRAFT/NEGOTIATING 状态的心愿
        let existing_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wishes \
             WHERE group_id = $1 AND status IN ('DRAFT'::wish_status_enum, 'NEGOTIATING'::wish_status_enum)",
        )
        .bind(group_id)
        .fetch_one(db)
        .await?;
        if existing_count > 0 {
            return Err(CustomError::BadRequest(
                "本组已存在协商中的心愿，请先处理后再创建".into(),
            ));
        }

        // 3. 创建心愿,requester_id = 创建人,fulfiller_id = 另一成员
        let rec = sqlx::query_as::<_, WishRecord>(
            "INSERT INTO wishes (wish_name, wish_cost, status, created_by, group_id, requester_id, fulfiller_id) \
             VALUES ($1, $2, 'NEGOTIATING'::wish_status_enum, $3, $4, $3, $5) \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(&input.wish_name)
        .bind(input.wish_cost)
        .bind(user_id as i64)
        .bind(group_id)
        .bind(fulfiller_id)
        .fetch_one(db)
        .await?;

        Ok(rec)
    }

    /// 获取心愿列表
    #[allow(dead_code)]
    pub async fn list_wishes(
        db: &PgPool,
        group_id: i64,
        _user_id: i64,
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

    /// 获取心愿详情（包含协商历史）
    pub async fn get_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<
        (
            WishRecord,
            Vec<WishNegotiationRecord>,
            Vec<WishFeedbackRecord>,
        ),
        CustomError,
    > {
        Self::auto_finish_overdue_wish(db, wish_id).await?;
        Self::auto_expire_overdue_wish(db, wish_id).await?;
        let rec = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, \
             claimed_by, claimed_at, claim_cost, created_at, updated_at, requester_id, fulfiller_id, \
             fulfilled_at, creator_checkin_due_at, auto_completed_at, points_frozen_at, finished_at \
             FROM wishes w \
             WHERE wish_id = $1 \
               AND (COALESCE(requester_id, created_by) = $2 OR fulfiller_id = $2 OR EXISTS ( \
                   SELECT 1 \
                   FROM association_group_members access_member \
                   JOIN association_groups access_group \
                     ON access_group.group_id = access_member.group_id \
                   WHERE access_member.group_id = w.group_id \
                     AND access_member.user_id = $2 \
                     AND access_member.member_status = 'ACTIVE'::group_member_status_enum \
                     AND access_group.status = 'ACTIVE'::user_status_enum \
               ))",
        )
        .bind(wish_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        let negotiations = sqlx::query_as::<_, WishNegotiationRecord>(
            "SELECT id, wish_id, group_id, operator_id, operator_role_snapshot, action::text AS action, cost, deadline_hours, remark, created_at \
             FROM wish_negotiations WHERE wish_id = $1 ORDER BY created_at ASC"
        )
        .bind(wish_id)
        .fetch_all(db)
        .await?;

        let feedbacks = Self::load_feedbacks(db, wish_id).await?;
        Ok((rec, negotiations, feedbacks))
    }

    /// 更新心愿
    pub async fn update_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishUpdateInput,
    ) -> Result<(WishRecord, Option<WishFeedbackRecord>), CustomError> {
        if let Some(wish_cost) = input.wish_cost {
            ensure_valid_wish_cost(wish_cost)?;
        }

        let mut tx = db.begin().await?;
        // 锁定心愿后，使用它的真实 group_id 校验写权限。
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

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
        .fetch_one(&mut *tx)
        .await?;

        let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT feedback_id, wish_id, user_id, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = $1",
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((rec, feedback))
    }

    /// 删除心愿
    pub async fn delete_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        let mut tx = db.begin().await?;
        // 获取并锁定原心愿
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        // 检查权限
        if existing.created_by != user_id as i64 {
            return Err(CustomError::Forbidden("无权删除此心愿".into()));
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLOSED'::wish_status_enum WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(wish_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(rec)
    }

    /// 提交心愿反馈
    pub async fn submit_feedback(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishFeedbackInput,
    ) -> Result<(WishRecord, Vec<WishFeedbackRecord>), CustomError> {
        let mut tx = db.begin().await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                    requester_id, fulfiller_id, fulfilled_at, creator_checkin_due_at, auto_completed_at \
             FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        let fulfiller_id = existing
            .fulfiller_id
            .or(existing.claimed_by)
            .ok_or_else(|| CustomError::BadRequest("心愿缺少履约方信息".into()))?;
        let role_snapshot = if user_id == requester_id {
            "REQUESTER"
        } else if user_id == fulfiller_id {
            "FULFILLER"
        } else {
            return Err(CustomError::Forbidden("无权提交此心愿反馈".into()));
        };

        if !matches!(existing.status, WishStatus::Claimed | WishStatus::Finished) {
            return Err(CustomError::BadRequest("当前状态不允许提交反馈".into()));
        }

        // 创建方必须在履约方打卡之后才能验收打卡；FINISHED 后双方仍可编辑自己的记录。
        if existing.status == WishStatus::Claimed && role_snapshot == "REQUESTER" {
            let fulfiller_checked_in: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM wish_feedbacks \
                 WHERE wish_id = $1 AND user_id = $2 AND role_snapshot = 'FULFILLER')",
            )
            .bind(wish_id)
            .bind(fulfiller_id)
            .fetch_one(&mut *tx)
            .await?;
            if !fulfiller_checked_in {
                return Err(CustomError::BadRequest(
                    "请等待履约人完成打卡后再验收".into(),
                ));
            }
        }

        let images_json = input.images.as_ref().map(|imgs| Json(imgs.clone()));
        sqlx::query(
            "INSERT INTO wish_feedbacks (wish_id, user_id, role_snapshot, content, images) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (wish_id, user_id) DO UPDATE \
             SET content = EXCLUDED.content, images = EXCLUDED.images, \
                 role_snapshot = EXCLUDED.role_snapshot, updated_at = NOW()",
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(role_snapshot)
        .bind(&input.content)
        .bind(images_json)
        .execute(&mut *tx)
        .await?;

        let mut completed_now = false;
        if existing.status == WishStatus::Claimed && role_snapshot == "FULFILLER" {
            sqlx::query(
                "UPDATE wishes SET fulfilled_at = COALESCE(fulfilled_at, NOW()), \
                        creator_checkin_due_at = COALESCE(creator_checkin_due_at, NOW() + INTERVAL '48 hours'), \
                        updated_at = NOW() WHERE wish_id = $1",
            )
            .bind(wish_id)
            .execute(&mut *tx)
            .await?;
        }

        if existing.status == WishStatus::Claimed {
            let feedback_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(DISTINCT user_id) FROM wish_feedbacks \
                 WHERE wish_id = $1 AND user_id IN ($2, $3)",
            )
            .bind(wish_id)
            .bind(requester_id)
            .bind(fulfiller_id)
            .fetch_one(&mut *tx)
            .await?;
            if feedback_count == 2 {
                Self::settle_wish_points_in_tx(&mut tx, wish_id, existing.group_id, requester_id)
                    .await?;
                sqlx::query(
                    "UPDATE wishes SET status = 'FINISHED'::wish_status_enum, \
                            finished_at = NOW(), creator_checkin_due_at = NULL, updated_at = NOW() \
                     WHERE wish_id = $1 AND status = 'CLAIMED'::wish_status_enum",
                )
                .bind(wish_id)
                .execute(&mut *tx)
                .await?;
                completed_now = true;
            }
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                    requester_id, fulfiller_id, fulfilled_at, creator_checkin_due_at, auto_completed_at, points_frozen_at, finished_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_one(&mut *tx)
        .await?;

        let feedbacks = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT feedback_id, wish_id, user_id, role_snapshot, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = $1 ORDER BY created_at ASC",
        )
        .bind(wish_id)
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;

        if completed_now {
            let _ = EventPublisher::publish(
                db,
                EventType::WishFinished,
                WishFinishedPayload {
                    wish_id,
                    requester_id,
                    fulfiller_id,
                    group_id: existing.group_id,
                    deducted_amount: existing.final_cost.unwrap_or(existing.wish_cost),
                    trace_id: None,
                },
                Some(user_id),
                Some(existing.group_id),
                Some("wish"),
                Some(wish_id),
            )
            .await;
        }

        Ok((rec, feedbacks))
    }

    // ============== FSD v2 心愿协商与选择接口 ==============

    /// 协商报价 - 发起人或履约人报价或还价
    /// 注意:DRAFT 状态已删除,创建直接进入 NEGOTIATING;首次报价视为首次报价,记录为 QUOTE;后续还价记 COUNTER
    pub async fn quote_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishQuoteInput,
    ) -> Result<WishRecord, CustomError> {
        ensure_valid_wish_cost(input.cost)?;

        let mut tx = db.begin().await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        // 检查状态:只有 NEGOTIATING 可以报价
        if !existing.status.is_negotiable() {
            return Err(CustomError::BadRequest("当前状态不允许报价".into()));
        }

        // 双方只有 requester/fulfiller 可以报价
        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        let fulfiller_id = existing.fulfiller_id.unwrap_or(0);
        if user_id != requester_id && user_id != fulfiller_id {
            return Err(CustomError::Forbidden("只有心愿协商双方可以报价".into()));
        }

        // P1-1:在报价/期限变更前,撤销之前所有 ACCEPT(它们绑定的是旧 final_cost)
        // PG 不会隐式把 text 转成自定义 enum,必须显式 ::wish_negotiation_action_enum
        sqlx::query(
            "DELETE FROM wish_negotiations WHERE wish_id = $1 AND action = 'ACCEPT'::wish_negotiation_action_enum"
        )
        .bind(wish_id)
        .execute(&mut *tx)
        .await?;

        // P1-2:并发竞态保护 - 必须等对方先动
        let last_actor: Option<i64> = sqlx::query_scalar(
            "SELECT operator_id FROM wish_negotiations WHERE wish_id = $1 ORDER BY created_at DESC LIMIT 1"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(last) = last_actor {
            if last == user_id {
                return Err(CustomError::BadRequest("请等待对方先回应再报价".into()));
            }
        }

        // P3-3: 操作者角色快照
        let role_snapshot = if user_id == requester_id {
            "REQUESTER"
        } else {
            "FULFILLER"
        };

        // 根据协商历史判断这是首条报价还是还价
        let has_any_negotiation: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_negotiations WHERE wish_id = $1 AND action IN ('QUOTE'::wish_negotiation_action_enum, 'COUNTER'::wish_negotiation_action_enum))"
        )
        .bind(wish_id)
        .fetch_one(&mut *tx)
        .await?;
        let action_label = if has_any_negotiation {
            "COUNTER"
        } else {
            "QUOTE"
        };

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, cost) VALUES ($1, $2, $3, $4, $5::wish_negotiation_action_enum, $6)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(role_snapshot)
        .bind(action_label)
        .bind(input.cost)
        .execute(&mut *tx)
        .await?;

        // 首条报价记录 initial_cost;后续报价更新 final_cost
        let rec = if !has_any_negotiation {
            sqlx::query_as::<_, WishRecord>(
                "UPDATE wishes SET initial_cost = $2, final_cost = $3, updated_at = NOW() WHERE wish_id = $1 \
                 RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                 requester_id, fulfiller_id, initial_cost, final_cost"
            )
            .bind(wish_id)
            .bind(input.cost)
            .bind(input.cost)
            .fetch_one(&mut *tx)
            .await
        } else {
            sqlx::query_as::<_, WishRecord>(
                "UPDATE wishes SET final_cost = $2, updated_at = NOW() WHERE wish_id = $1 \
                 RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                 requester_id, fulfiller_id, initial_cost, final_cost"
            )
            .bind(wish_id)
            .bind(input.cost)
            .fetch_one(&mut *tx)
            .await
        }
        .map_err(|e| CustomError::internal(e.to_string()))?;

        tx.commit().await?;

        // P1-3: 发布报价事件通知对方
        let _ = EventPublisher::publish(
            db,
            EventType::WishNegotiating,
            WishNegotiatingPayload {
                wish_id,
                operator_id: user_id,
                group_id: existing.group_id,
                action: action_label.into(),
                cost: Some(input.cost),
                deadline_hours: None,
                trace_id: None,
            },
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;

        Ok(rec)
    }

    /// 协商履约期限
    pub async fn set_deadline(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishDeadlineInput,
    ) -> Result<WishRecord, CustomError> {
        let mut tx = db.begin().await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        if !existing.status.is_negotiable() {
            return Err(CustomError::BadRequest("当前状态不允许设置期限".into()));
        }

        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        let fulfiller_id = existing.fulfiller_id.unwrap_or(0);
        if user_id != requester_id && user_id != fulfiller_id {
            return Err(CustomError::Forbidden(
                "只有心愿协商双方可以设置期限".into(),
            ));
        }

        // P1-1:期限变更 → 撤销之前的 ACCEPT(旧 ACCEPT 绑定的是旧 deadline)
        sqlx::query(
            "DELETE FROM wish_negotiations WHERE wish_id = $1 AND action = 'ACCEPT'::wish_negotiation_action_enum"
        )
        .bind(wish_id)
        .execute(&mut *tx)
        .await?;

        // P1-2:并发竞态保护 - 必须等对方先动
        let last_actor: Option<i64> = sqlx::query_scalar(
            "SELECT operator_id FROM wish_negotiations WHERE wish_id = $1 ORDER BY created_at DESC LIMIT 1"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(last) = last_actor {
            if last == user_id {
                return Err(CustomError::BadRequest("请等待对方先回应再调整期限".into()));
            }
        }

        // P3-3: 操作者角色快照
        let role_snapshot = if user_id == requester_id {
            "REQUESTER"
        } else {
            "FULFILLER"
        };

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, deadline_hours) VALUES ($1, $2, $3, $4, 'SET_DEADLINE'::wish_negotiation_action_enum, $5)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(role_snapshot)
        .bind(input.deadline_hours)
        .execute(&mut *tx)
        .await?;

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET fulfillment_deadline_hours = $2, updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .bind(input.deadline_hours)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        // P1-3:发布期限变更事件
        let _ = EventPublisher::publish(
            db,
            EventType::WishNegotiating,
            WishNegotiatingPayload {
                wish_id,
                operator_id: user_id,
                group_id: existing.group_id,
                action: "SET_DEADLINE".into(),
                cost: None,
                deadline_hours: Some(input.deadline_hours),
                trace_id: None,
            },
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;

        Ok(rec)
    }

    /// 双方线上确认积分和期限，心愿进入 CREATED 状态（心愿池）
    pub async fn confirm_agreement(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<crate::domain::wish::entities::WishOut, CustomError> {
        let mut tx = db.begin().await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::wish_not_found("心愿不存在"))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        if existing.status != WishStatus::Negotiating {
            return Err(CustomError::wish_status_invalid("当前状态不允许确认"));
        }

        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        let fulfiller_id = existing.fulfiller_id.unwrap_or(0);
        if user_id != requester_id && user_id != fulfiller_id {
            return Err(CustomError::Forbidden("只有心愿协商双方可以确认".into()));
        }

        // 确认后使用 final_cost 作为最终积分
        let final_cost = existing
            .final_cost
            .or(existing.initial_cost)
            .unwrap_or(existing.wish_cost);
        ensure_valid_wish_cost(final_cost)?;

        // P1-1 修复 + 幂等保护:同一用户多次点击 ACCEPT 只保留一条
        sqlx::query(
            "DELETE FROM wish_negotiations WHERE wish_id = $1 AND operator_id = $2 AND action = 'ACCEPT'::wish_negotiation_action_enum AND cost <> $3"
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(final_cost)
        .execute(&mut *tx)
        .await?;

        // 幂等:同一 cost 下不重复插入 ACCEPT
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_negotiations WHERE wish_id = $1 AND operator_id = $2 AND action = 'ACCEPT'::wish_negotiation_action_enum AND cost = $3)"
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(final_cost)
        .fetch_one(&mut *tx)
        .await?;

        if !exists {
            // P3-3: 操作者角色快照
            let role_snapshot = if user_id == requester_id {
                "REQUESTER"
            } else {
                "FULFILLER"
            };
            sqlx::query(
                "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, cost) VALUES ($1, $2, $3, $4, 'ACCEPT'::wish_negotiation_action_enum, $5)"
            )
            .bind(wish_id)
            .bind(existing.group_id)
            .bind(user_id)
            .bind(role_snapshot)
            .bind(final_cost)
            .execute(&mut *tx)
            .await?;
        }

        // 2) 取「双方都 ACCEPT」的最新状态(P1-1:基于当前 final_cost 的 ACCEPT)
        let confirmed_rows = sqlx::query(
            "SELECT DISTINCT operator_id FROM wish_negotiations WHERE wish_id = $1 AND action = 'ACCEPT'::wish_negotiation_action_enum AND cost = $2"
        )
        .bind(wish_id)
        .bind(final_cost)
        .fetch_all(&mut *tx)
        .await?;
        let mut confirmed_set = std::collections::HashSet::new();
        for r in confirmed_rows {
            confirmed_set.insert(r.get::<i64, _>("operator_id"));
        }

        // 3) 单边 ACCEPT:返回 WishOut 并附带 negotiation_status,前端无需再调详情
        if !confirmed_set.contains(&requester_id) || !confirmed_set.contains(&fulfiller_id) {
            let rec = sqlx::query_as::<_, WishRecord>(
                "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                 requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours \
                 FROM wishes WHERE wish_id = $1"
            )
            .bind(wish_id)
            .fetch_one(&mut *tx)
            .await?;

            tx.commit().await?;

            let _ = EventPublisher::publish(
                db,
                EventType::WishNegotiating,
                WishNegotiatingPayload {
                    wish_id,
                    operator_id: user_id,
                    group_id: existing.group_id,
                    action: "ACCEPT".into(),
                    cost: Some(final_cost),
                    deadline_hours: None,
                    trace_id: None,
                },
                Some(user_id),
                Some(existing.group_id),
                Some("wish"),
                Some(wish_id),
            )
            .await;

            let other_party = if user_id == requester_id {
                fulfiller_id
            } else {
                requester_id
            };
            let mut out = crate::domain::wish::entities::WishOut::from_record(rec, None);
            out.negotiation_status = Some(crate::domain::wish::entities::WishNegotiationStatus {
                i_accepted: true,
                they_accepted: confirmed_set.contains(&other_party),
                awaiting_party: if !confirmed_set.contains(&requester_id) {
                    Some("REQUESTER".to_string())
                } else if !confirmed_set.contains(&fulfiller_id) {
                    Some("FULFILLER".to_string())
                } else {
                    None
                },
                agreed: false,
            });
            return Ok(out);
        }

        // 4) 双方都 ACCEPT → 冻结创建人的积分并进入心愿池。
        Self::freeze_wish_points_in_tx(
            &mut tx,
            wish_id,
            existing.group_id,
            requester_id,
            final_cost,
        )
        .await?;

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CREATED'::wish_status_enum, wish_cost = $2, \
                    points_frozen_at = COALESCE(points_frozen_at, NOW()), updated_at = NOW() \
             WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours, \
             points_frozen_at, creator_checkin_due_at, auto_completed_at, fulfilled_at"
        )
        .bind(wish_id)
        .bind(final_cost)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        let _ = EventPublisher::publish(
            db,
            EventType::WishAgreementConfirmed,
            WishAgreementConfirmedPayload {
                wish_id,
                requester_id,
                fulfiller_id,
                group_id: existing.group_id,
                final_cost,
                fulfillment_deadline_hours: existing.fulfillment_deadline_hours.unwrap_or(72),
                trace_id: None,
            },
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;

        // 双方都同意,返回 agreed=true
        let mut out = crate::domain::wish::entities::WishOut::from_record(rec, None);
        out.negotiation_status = Some(crate::domain::wish::entities::WishNegotiationStatus {
            i_accepted: true,
            they_accepted: true,
            awaiting_party: None,
            agreed: true,
        });
        Ok(out)
    }

    /// 拒绝协商中的心愿。
    pub async fn reject_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishRejectInput,
    ) -> Result<WishRecord, CustomError> {
        Self::reject_or_close_wish(db, user_id, wish_id, input, WishClosureAction::Reject).await
    }

    /// 关闭非终态心愿；CLAIMED 心愿会在同一事务内解冻积分。
    pub async fn close_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishRejectInput,
    ) -> Result<WishRecord, CustomError> {
        Self::reject_or_close_wish(db, user_id, wish_id, input, WishClosureAction::Close).await
    }

    /// REJECT/CLOSE 共用事务：锁定心愿、校验参与方与状态、条件更新、退款并记协商流水。
    async fn reject_or_close_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishRejectInput,
        action: WishClosureAction,
    ) -> Result<WishRecord, CustomError> {
        let mut tx = db.begin().await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours \
             FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        let requester_id = existing
            .requester_id
            .ok_or_else(|| CustomError::BadRequest("心愿缺少发起方信息".into()))?;
        let fulfiller_id = existing
            .fulfiller_id
            .ok_or_else(|| CustomError::BadRequest("心愿缺少履约方信息".into()))?;
        if action == WishClosureAction::Close
            && existing.status == WishStatus::Created
            && user_id != requester_id
        {
            return Err(CustomError::Forbidden(
                "只有心愿创建人可以关闭尚未领取的心愿".into(),
            ));
        }
        let role_snapshot = match validate_wish_closure_policy(
            existing.status,
            action,
            user_id,
            requester_id,
            fulfiller_id,
        ) {
            Ok(role) => role,
            Err(WishClosurePolicyError::NotParticipant) => {
                return Err(CustomError::Forbidden(
                    "只有心愿的发起方或履约方可以拒绝或关闭".into(),
                ));
            }
            Err(WishClosurePolicyError::InvalidStatus) => {
                let message = match action {
                    WishClosureAction::Reject => {
                        "协商拒绝仅在 NEGOTIATING 状态可用,关闭请用 /close 接口"
                    }
                    WishClosureAction::Close => "只有非终态心愿可以关闭",
                };
                return Err(CustomError::BadRequest(message.into()));
            }
        };

        // 条件更新先于退款；即使状态被数据库触发器等并发机制改写，也不会留下退款和旧状态。
        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes \
             SET status = 'CLOSED'::wish_status_enum, closed_at = NOW(), updated_at = NOW() \
             WHERE wish_id = $1 AND status = $2 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .bind(existing.status)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            CustomError::BadRequest("心愿状态已变化,请刷新后重试".into())
        })?;

        let should_unfreeze =
            action == WishClosureAction::Close && existing.status == WishStatus::Created;
        if should_unfreeze {
            let idempotency_key = format!("wish_close_{wish_id}");
            Self::unfreeze_wish_points_in_tx(
                &mut tx,
                wish_id,
                existing.group_id,
                requester_id,
                &idempotency_key,
            )
            .await?;
        }

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, remark) \
             VALUES ($1, $2, $3, $4, $5::wish_negotiation_action_enum, $6)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(role_snapshot)
        .bind(action.negotiation_label())
        .bind(&input.reason)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let _ = EventPublisher::publish(
            db,
            EventType::WishClosed,
            WishClosedPayload {
                wish_id,
                operator_id: user_id,
                group_id: existing.group_id,
                reason: input.reason.clone(),
                unfrozen_if_any: should_unfreeze,
                trace_id: None,
            },
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;

        Ok(rec)
    }

    /// 在调用方事务内解冻心愿积分。
    ///
    /// 先锁定 users 行，使同一用户的 FREEZE/UNFREEZE 串行执行；流水插入同时依赖
    /// 部分唯一索引兜底，只有成功写入 UNFREEZE 流水的事务才会恢复真实余额。
    async fn unfreeze_wish_points_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        wish_id: i64,
        group_id: i64,
        requester_id: i64,
        idempotency_key: &str,
    ) -> Result<i64, CustomError> {
        let available_before: i32 =
            sqlx::query_scalar("SELECT love_point FROM users WHERE user_id = $1 FOR UPDATE")
                .bind(requester_id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| CustomError::NotFound("心愿发起人不存在".into()))?;

        let already_unfrozen: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM love_point_transactions WHERE idempotency_key = $1 LIMIT 1",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut **tx)
        .await?;
        if already_unfrozen.is_some() {
            return Ok(0);
        }

        let frozen_amount: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(
                   SUM(CASE
                       WHEN type = 'FREEZE'::love_point_tx_type_enum THEN amount
                       WHEN type IN (
                           'UNFREEZE'::love_point_tx_type_enum,
                           'DEDUCT'::love_point_tx_type_enum
                       ) THEN -amount
                       ELSE 0
                   END),
                   0
               )::bigint
               FROM love_point_transactions
               WHERE user_id = $1 AND group_id = $2 AND biz_id = $3 AND biz_type = 'wish'"#,
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(wish_id)
        .fetch_one(&mut **tx)
        .await?;
        if frozen_amount <= 0 {
            return Ok(0);
        }

        let refund = i32::try_from(frozen_amount)
            .map_err(|_| CustomError::internal("心愿冻结积分超出支持范围"))?;
        let available_after = available_before
            .checked_add(refund)
            .ok_or_else(|| CustomError::internal("用户爱心积分溢出"))?;

        let group_frozen_before: Option<i64> = sqlx::query_scalar(
            "SELECT frozen_love_point FROM user_group_points \
             WHERE user_id = $1 AND group_id = $2 FOR UPDATE",
        )
        .bind(requester_id)
        .bind(group_id)
        .fetch_optional(&mut **tx)
        .await?;
        let frozen_before = match group_frozen_before {
            Some(value) => value,
            None => {
                sqlx::query_scalar(
                    r#"SELECT COALESCE(
                           SUM(CASE
                               WHEN type = 'FREEZE'::love_point_tx_type_enum THEN amount
                               WHEN type IN (
                                   'UNFREEZE'::love_point_tx_type_enum,
                                   'DEDUCT'::love_point_tx_type_enum
                               ) THEN -amount
                               ELSE 0
                           END),
                           0
                       )::bigint
                       FROM love_point_transactions
                       WHERE user_id = $1 AND group_id = $2"#,
                )
                .bind(requester_id)
                .bind(group_id)
                .fetch_one(&mut **tx)
                .await?
            }
        };
        let frozen_after = frozen_before.saturating_sub(frozen_amount).max(0);

        let inserted: Option<i64> = sqlx::query_scalar(
            r#"INSERT INTO love_point_transactions
                   (user_id, group_id, type, amount, available_before, available_after,
                    frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at)
               VALUES
                   ($1, $2, 'UNFREEZE'::love_point_tx_type_enum, $3, $4, $5,
                    $6, $7, 'wish', $8, $9, NOW())
               ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL
               DO NOTHING
               RETURNING id"#,
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(frozen_amount)
        .bind(available_before)
        .bind(available_after)
        .bind(frozen_before)
        .bind(frozen_after)
        .bind(wish_id)
        .bind(idempotency_key)
        .fetch_optional(&mut **tx)
        .await?;
        if inserted.is_none() {
            return Ok(0);
        }

        sqlx::query("UPDATE users SET love_point = $2 WHERE user_id = $1")
            .bind(requester_id)
            .bind(available_after)
            .execute(&mut **tx)
            .await?;

        sqlx::query(
            "INSERT INTO user_group_points \
                 (user_id, group_id, available_love_point, love_point, frozen_love_point, updated_at) \
             VALUES ($1, $2, $3, $3, 0, NOW()) \
             ON CONFLICT (user_id, group_id) DO UPDATE \
             SET available_love_point = $3, \
                 love_point = $3, \
                 frozen_love_point = GREATEST(user_group_points.frozen_love_point - $4, 0), \
                 updated_at = NOW()",
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(i64::from(available_after))
        .bind(frozen_amount)
        .execute(&mut **tx)
        .await?;

        Ok(frozen_amount)
    }

    /// 解冻心愿冻结积分，供独立业务流程复用。
    pub async fn unfreeze_wish_points(
        db: &PgPool,
        wish_id: i64,
        group_id: i64,
        requester_id: i64,
        idempotency_key: &str,
    ) -> Result<i64, CustomError> {
        let mut tx = db.begin().await?;
        let frozen_amount = Self::unfreeze_wish_points_in_tx(
            &mut tx,
            wish_id,
            group_id,
            requester_id,
            idempotency_key,
        )
        .await?;
        tx.commit().await?;
        Ok(frozen_amount)
    }

    /// 将已到期的 CLAIMED 心愿标记为 EXPIRED，并在同一事务内真实退还冻结积分。
    pub async fn expire_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishExpireResult, CustomError> {
        if Self::auto_finish_overdue_wish(db, wish_id).await? {
            return Err(CustomError::BadRequest(
                "履约人已打卡，心愿已自动完成".into(),
            ));
        }
        let mut tx = db.begin().await?;
        let existing: (
            String,
            Option<i64>,
            Option<i64>,
            i64,
            Option<DateTime<Utc>>,
            bool,
        ) = sqlx::query_as(
            "SELECT status::text, requester_id, fulfiller_id, group_id, fulfillment_due_at, \
                    COALESCE(fulfillment_due_at <= NOW(), false) AS is_due \
             FROM wishes WHERE wish_id = $1 FOR UPDATE",
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        let (status, requester_id, fulfiller_id, group_id, fulfillment_due_at, is_due) = existing;
        ensure_active_wish_actor_in_tx(&mut tx, group_id, user_id).await?;
        let requester_id =
            requester_id.ok_or_else(|| CustomError::BadRequest("心愿缺少发起方信息".into()))?;
        let fulfiller_id =
            fulfiller_id.ok_or_else(|| CustomError::BadRequest("心愿缺少履约方信息".into()))?;

        if user_id != requester_id && user_id != fulfiller_id {
            return Err(CustomError::Forbidden(
                "只有心愿的发起方或履约方可以处理逾期".into(),
            ));
        }

        if status == "EXPIRED" {
            tx.commit().await?;
            return Ok(WishExpireResult {
                unfrozen_amount: 0,
                already_processed: true,
            });
        }
        if status != "CLAIMED" {
            return Err(CustomError::BadRequest("心愿状态不允许逾期处理".into()));
        }
        let fulfiller_checked_in: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_feedbacks \
             WHERE wish_id = $1 AND user_id = $2 AND role_snapshot = 'FULFILLER')",
        )
        .bind(wish_id)
        .bind(fulfiller_id)
        .fetch_one(&mut *tx)
        .await?;
        if fulfiller_checked_in {
            return Err(CustomError::BadRequest(
                "履约人已打卡，正在等待创建人验收".into(),
            ));
        }
        if fulfillment_due_at.is_none() || !is_due {
            return Err(CustomError::BadRequest("心愿履约期限尚未到期".into()));
        }

        let updated: Option<i64> = sqlx::query_scalar(
            "UPDATE wishes \
             SET status = 'EXPIRED'::wish_status_enum, expired_at = NOW(), updated_at = NOW() \
             WHERE wish_id = $1 \
               AND status = 'CLAIMED'::wish_status_enum \
               AND fulfillment_due_at <= NOW() \
             RETURNING wish_id",
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?;
        if updated.is_none() {
            return Err(CustomError::BadRequest("心愿状态不允许逾期处理".into()));
        }

        let idempotency_key = format!("wish_expire_{wish_id}");
        let unfrozen_amount = Self::unfreeze_wish_points_in_tx(
            &mut tx,
            wish_id,
            group_id,
            requester_id,
            &idempotency_key,
        )
        .await?;
        let event_unfrozen_amount = i32::try_from(unfrozen_amount)
            .map_err(|_| CustomError::internal("心愿解冻积分超出支持范围"))?;

        tx.commit().await?;

        let _ = EventPublisher::publish(
            db,
            EventType::WishExpired,
            WishExpiredPayload {
                wish_id,
                requester_id,
                fulfiller_id,
                group_id,
                unfrozen_amount: event_unfrozen_amount,
                trace_id: None,
            },
            Some(user_id),
            Some(group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;

        Ok(WishExpireResult {
            unfrozen_amount,
            already_processed: false,
        })
    }

    /// 履约人从心愿池领取心愿；积分已在双方同意进入心愿池时冻结。
    pub async fn select_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        let mut tx = db.begin().await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, fulfillment_due_at, fulfillment_deadline_hours, points_frozen_at \
             FROM wishes WHERE wish_id = $1 FOR UPDATE"
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        if existing.status != WishStatus::Created {
            return Err(CustomError::BadRequest("心愿当前不可选择".into()));
        }

        if existing.claimed_by.is_some() {
            return Err(CustomError::BadRequest("心愿已被选择".into()));
        }

        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        let fulfiller_id = existing
            .fulfiller_id
            .ok_or_else(|| CustomError::BadRequest("心愿缺少履约方信息".into()))?;
        if user_id != fulfiller_id {
            return Err(CustomError::Forbidden("只有指定履约人可以领取心愿".into()));
        }

        let points_cost = existing.final_cost.unwrap_or(existing.wish_cost);
        ensure_valid_wish_cost(points_cost)?;

        // 兼容升级前已在心愿池、但尚未冻结积分的历史 CREATED 数据。
        if existing.points_frozen_at.is_none() {
            Self::freeze_wish_points_in_tx(
                &mut tx,
                wish_id,
                existing.group_id,
                requester_id,
                points_cost,
            )
            .await?;
        }

        // 同一履约人的领取请求串行化，配合部分唯一索引避免并发领取两条。
        sqlx::query_scalar::<_, i64>("SELECT user_id FROM users WHERE user_id = $1 FOR UPDATE")
            .bind(fulfiller_id)
            .fetch_one(&mut *tx)
            .await?;
        let has_active_claim: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wishes \
             WHERE group_id = $1 AND claimed_by = $2 AND status = 'CLAIMED'::wish_status_enum)",
        )
        .bind(existing.group_id)
        .bind(fulfiller_id)
        .fetch_one(&mut *tx)
        .await?;
        if has_active_claim {
            return Err(CustomError::BadRequest(
                "你已有履约中的心愿，请先完成或放弃领取".into(),
            ));
        }

        // 计算履约截止时间
        let deadline_hours = existing.fulfillment_deadline_hours.unwrap_or(72);
        let fulfillment_due_at = Utc::now() + Duration::hours(deadline_hours as i64);

        // 更新心愿状态为 CLAIMED(条件 UPDATE 防止 TOCTOU 重复 select)
        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLAIMED'::wish_status_enum, selected_by = $2, \
                    selected_at = NOW(), claimed_by = $2, claimed_at = NOW(), \
                    fulfillment_due_at = $3, claim_cost = $4, \
                    points_frozen_at = COALESCE(points_frozen_at, NOW()), updated_at = NOW() \
             WHERE wish_id = $1 AND status = 'CREATED'::wish_status_enum AND claimed_by IS NULL \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, fulfillment_due_at, fulfillment_deadline_hours, \
             points_frozen_at, creator_checkin_due_at, auto_completed_at, fulfilled_at"
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(fulfillment_due_at)
        .bind(points_cost)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::BadRequest("心愿已被选择或状态变更".into()))?;

        tx.commit().await?;

        // 发布事件
        let _ = EventPublisher::publish(
            db,
            EventType::WishSelected,
            WishSelectedPayload {
                wish_id,
                requester_id,
                fulfiller_id,
                group_id: existing.group_id,
                frozen_amount: points_cost,
                fulfillment_due_at: fulfillment_due_at.to_rfc3339(),
                trace_id: None,
            },
            Some(user_id),
            Some(existing.group_id),
            Some("wish"),
            Some(wish_id),
        )
        .await;

        Ok(rec)
    }

    /// 履约人在尚未打卡时放弃领取，心愿回到池中；创建人的积分继续保持冻结。
    pub async fn release_claim(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        let mut tx = db.begin().await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id FROM wishes WHERE wish_id = $1 FOR UPDATE",
        )
        .bind(wish_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::wish_not_found("心愿不存在"))?;
        ensure_active_wish_actor_in_tx(&mut tx, existing.group_id, user_id).await?;

        if existing.status != WishStatus::Claimed || existing.claimed_by != Some(user_id) {
            return Err(CustomError::Forbidden("只有当前履约人可以放弃领取".into()));
        }
        let feedback_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM wish_feedbacks WHERE wish_id = $1")
                .bind(wish_id)
                .fetch_one(&mut *tx)
                .await?;
        if feedback_count > 0 {
            return Err(CustomError::BadRequest("已有打卡记录，不能放弃领取".into()));
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CREATED'::wish_status_enum, \
                    selected_by = NULL, selected_at = NULL, claimed_by = NULL, claimed_at = NULL, \
                    claim_cost = NULL, fulfillment_due_at = NULL, fulfilled_at = NULL, \
                    creator_checkin_due_at = NULL, updated_at = NOW() \
             WHERE wish_id = $1 AND status = 'CLAIMED'::wish_status_enum \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, points_frozen_at, creator_checkin_due_at, auto_completed_at, fulfilled_at",
        )
        .bind(wish_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rec)
    }

    /// 兼容旧客户端的确认端点。新流程由双方反馈自动完成，不再允许单独确认。
    pub async fn confirm_wish_completion(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        Self::auto_finish_overdue_wish(db, wish_id).await?;
        Self::auto_expire_overdue_wish(db, wish_id).await?;
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours, \
             fulfilled_at, creator_checkin_due_at, auto_completed_at, points_frozen_at, finished_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;
        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        let fulfiller_id = existing.fulfiller_id.or(existing.claimed_by).unwrap_or(0);
        if user_id != requester_id && user_id != fulfiller_id {
            return Err(CustomError::Forbidden("无权操作此心愿".into()));
        }
        if existing.status == WishStatus::Finished {
            return Ok(existing);
        }
        Err(CustomError::BadRequest(
            "新流程需要双方完成打卡后自动完成心愿".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_active_wish_actor_snapshot, validate_wish_closure_policy, WishActorAccessError,
        WishClosureAction, WishClosurePolicyError, WishStatus,
    };

    const REQUESTER_ID: i64 = 11;
    const FULFILLER_ID: i64 = 22;
    const OUTSIDER_ID: i64 = 33;

    #[test]
    fn active_wish_actor_snapshot_requires_both_active_states() {
        assert_eq!(
            validate_active_wish_actor_snapshot(Some("ACTIVE"), Some("ACTIVE")),
            Ok(())
        );
        assert_eq!(
            validate_active_wish_actor_snapshot(Some("DISABLED"), Some("ACTIVE")),
            Err(WishActorAccessError::GroupInactive)
        );
        assert_eq!(
            validate_active_wish_actor_snapshot(None, Some("ACTIVE")),
            Err(WishActorAccessError::GroupInactive)
        );
        assert_eq!(
            validate_active_wish_actor_snapshot(Some("ACTIVE"), Some("LEFT")),
            Err(WishActorAccessError::MemberInactive)
        );
        assert_eq!(
            validate_active_wish_actor_snapshot(Some("ACTIVE"), None),
            Err(WishActorAccessError::MemberInactive)
        );
    }

    #[test]
    fn close_and_reject_reject_non_participants() {
        for action in [WishClosureAction::Reject, WishClosureAction::Close] {
            assert_eq!(
                validate_wish_closure_policy(
                    WishStatus::Negotiating,
                    action,
                    OUTSIDER_ID,
                    REQUESTER_ID,
                    FULFILLER_ID,
                ),
                Err(WishClosurePolicyError::NotParticipant)
            );
        }
    }

    #[test]
    fn closure_policy_identifies_both_participant_roles() {
        assert_eq!(
            validate_wish_closure_policy(
                WishStatus::Negotiating,
                WishClosureAction::Reject,
                REQUESTER_ID,
                REQUESTER_ID,
                FULFILLER_ID,
            ),
            Ok("REQUESTER")
        );
        assert_eq!(
            validate_wish_closure_policy(
                WishStatus::Created,
                WishClosureAction::Close,
                FULFILLER_ID,
                REQUESTER_ID,
                FULFILLER_ID,
            ),
            Ok("FULFILLER")
        );
    }

    #[test]
    fn reject_policy_only_allows_negotiating() {
        for status in [
            WishStatus::Created,
            WishStatus::Claimed,
            WishStatus::Finished,
            WishStatus::Expired,
            WishStatus::Closed,
        ] {
            assert_eq!(
                validate_wish_closure_policy(
                    status,
                    WishClosureAction::Reject,
                    REQUESTER_ID,
                    REQUESTER_ID,
                    FULFILLER_ID,
                ),
                Err(WishClosurePolicyError::InvalidStatus)
            );
        }
        assert!(validate_wish_closure_policy(
            WishStatus::Negotiating,
            WishClosureAction::Reject,
            REQUESTER_ID,
            REQUESTER_ID,
            FULFILLER_ID,
        )
        .is_ok());
    }

    #[test]
    fn close_policy_disallows_unilateral_close_after_claim() {
        for status in [WishStatus::Negotiating, WishStatus::Created] {
            assert!(validate_wish_closure_policy(
                status,
                WishClosureAction::Close,
                REQUESTER_ID,
                REQUESTER_ID,
                FULFILLER_ID,
            )
            .is_ok());
        }
        for status in [
            WishStatus::Claimed,
            WishStatus::Finished,
            WishStatus::Expired,
            WishStatus::Closed,
        ] {
            assert_eq!(
                validate_wish_closure_policy(
                    status,
                    WishClosureAction::Close,
                    REQUESTER_ID,
                    REQUESTER_ID,
                    FULFILLER_ID,
                ),
                Err(WishClosurePolicyError::InvalidStatus)
            );
        }
    }
}
