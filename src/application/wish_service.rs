// 应用服务层 - 心愿服务
// 包含心愿创建、认领、兑换、反馈等业务用例
// FSD.latest.md compliant - 7状态模型

use chrono::{DateTime, Duration, Utc};
use sqlx::types::Json;
use sqlx::{PgPool, Row};

use crate::domain::event::{
    EventType, WishAgreementConfirmedPayload, WishClosedPayload,
    WishFinishedPayload, WishNegotiatingPayload, WishSelectedPayload,
};
use crate::domain::wish::{
    WishCreateInput, WishDeadlineInput, WishFeedbackInput, WishFeedbackRecord,
    WishNegotiationRecord, WishQuoteInput, WishRecord, WishRejectInput, WishStatus,
    WishUpdateInput,
};
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
        .bind(input.group_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
        let fulfiller_id = other_id.ok_or_else(|| {
            CustomError::BadRequest("组内需要至少 2 名成员才能创建心愿".into())
        })?;

        // 2. 同组同时只能有 1 条 DRAFT/NEGOTIATING 状态的心愿
        let existing_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wishes \
             WHERE group_id = $1 AND status IN ('DRAFT'::wish_status_enum, 'NEGOTIATING'::wish_status_enum)",
        )
        .bind(input.group_id)
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
        .bind(input.group_id)
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
        wish_id: i64,
    ) -> Result<(WishRecord, Vec<WishNegotiationRecord>), CustomError> {
        let rec = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, \
             claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
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

        Ok((rec, negotiations))
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
             FROM wish_feedbacks WHERE wish_id = $1",
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
            "UPDATE wishes SET status = 'CLOSED'::wish_status_enum WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

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
        if existing.created_by != user_id as i64
            && existing
                .claimed_by
                .map(|c| c != user_id as i64)
                .unwrap_or(true)
        {
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
             ON CONFLICT (wish_id) DO UPDATE SET content = $3, images = $4",
        )
        .bind(wish_id)
        .bind(user_id as i64)
        .bind(&input.content)
        .bind(images_json)
        .execute(db)
        .await?;

        // 状态保持 CLAIMED —— 由接单人 confirm-completion 才推进到 FINISHED
        // 这里只读心愿最新状态返回,不改任何字段
        let rec = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at \
             FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        // 获取反馈记录
        let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT feedback_id, wish_id, user_id, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = $1",
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;

        Ok((rec, feedback))
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
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

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
        .execute(db)
        .await?;

        // P1-2:并发竞态保护 - 必须等对方先动
        let last_actor: Option<i64> = sqlx::query_scalar(
            "SELECT operator_id FROM wish_negotiations WHERE wish_id = $1 ORDER BY created_at DESC LIMIT 1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;
        if let Some(last) = last_actor {
            if last == user_id {
                return Err(CustomError::BadRequest(
                    "请等待对方先回应再报价".into(),
                ));
            }
        }

        // P3-3: 操作者角色快照
        let role_snapshot = if user_id == requester_id { "REQUESTER" } else { "FULFILLER" };

        // 根据协商历史判断这是首条报价还是还价
        let has_any_negotiation: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_negotiations WHERE wish_id = $1 AND action IN ('QUOTE'::wish_negotiation_action_enum, 'COUNTER'::wish_negotiation_action_enum))"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;
        let action_label = if has_any_negotiation { "COUNTER" } else { "QUOTE" };

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, cost) VALUES ($1, $2, $3, $4, $5::wish_negotiation_action_enum, $6)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(role_snapshot)
        .bind(action_label)
        .bind(input.cost)
        .execute(db)
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
            .fetch_one(db)
            .await
        } else {
            sqlx::query_as::<_, WishRecord>(
                "UPDATE wishes SET final_cost = $2, updated_at = NOW() WHERE wish_id = $1 \
                 RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
                 requester_id, fulfiller_id, initial_cost, final_cost"
            )
            .bind(wish_id)
            .bind(input.cost)
            .fetch_one(db)
            .await
        }
        .map_err(|e| CustomError::internal(e.to_string()))?;

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

        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        let fulfiller_id = existing.fulfiller_id.unwrap_or(0);
        if user_id != requester_id && user_id != fulfiller_id {
            return Err(CustomError::Forbidden("只有心愿协商双方可以设置期限".into()));
        }

        // P1-1:期限变更 → 撤销之前的 ACCEPT(旧 ACCEPT 绑定的是旧 deadline)
        sqlx::query(
            "DELETE FROM wish_negotiations WHERE wish_id = $1 AND action = 'ACCEPT'::wish_negotiation_action_enum"
        )
        .bind(wish_id)
        .execute(db)
        .await?;

        // P1-2:并发竞态保护 - 必须等对方先动
        let last_actor: Option<i64> = sqlx::query_scalar(
            "SELECT operator_id FROM wish_negotiations WHERE wish_id = $1 ORDER BY created_at DESC LIMIT 1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;
        if let Some(last) = last_actor {
            if last == user_id {
                return Err(CustomError::BadRequest(
                    "请等待对方先回应再调整期限".into(),
                ));
            }
        }

        // P3-3: 操作者角色快照
        let role_snapshot = if user_id == requester_id { "REQUESTER" } else { "FULFILLER" };

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, deadline_hours) VALUES ($1, $2, $3, $4, 'SET_DEADLINE'::wish_negotiation_action_enum, $5)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(role_snapshot)
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
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::wish_not_found("心愿不存在"))?;

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

        // P1-1 修复 + 幂等保护:同一用户多次点击 ACCEPT 只保留一条
        sqlx::query(
            "DELETE FROM wish_negotiations WHERE wish_id = $1 AND operator_id = $2 AND action = 'ACCEPT'::wish_negotiation_action_enum AND cost <> $3"
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(final_cost)
        .execute(db)
        .await?;

        // 幂等:同一 cost 下不重复插入 ACCEPT
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_negotiations WHERE wish_id = $1 AND operator_id = $2 AND action = 'ACCEPT'::wish_negotiation_action_enum AND cost = $3)"
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(final_cost)
        .fetch_one(db)
        .await?;

        if !exists {
            // P3-3: 操作者角色快照
            let role_snapshot = if user_id == requester_id { "REQUESTER" } else { "FULFILLER" };
            sqlx::query(
                "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, cost) VALUES ($1, $2, $3, $4, 'ACCEPT'::wish_negotiation_action_enum, $5)"
            )
            .bind(wish_id)
            .bind(existing.group_id)
            .bind(user_id)
            .bind(role_snapshot)
            .bind(final_cost)
            .execute(db)
            .await?;
        }

        // 2) 取「双方都 ACCEPT」的最新状态(P1-1:基于当前 final_cost 的 ACCEPT)
        let confirmed_rows = sqlx::query(
            "SELECT DISTINCT operator_id FROM wish_negotiations WHERE wish_id = $1 AND action = 'ACCEPT'::wish_negotiation_action_enum AND cost = $2"
        )
        .bind(wish_id)
        .bind(final_cost)
        .fetch_all(db)
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
            .fetch_one(db)
            .await?;

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

            let other_party = if user_id == requester_id { fulfiller_id } else { requester_id };
            let mut out = crate::domain::wish::entities::WishOut::from_record(rec, None);
            out.negotiation_status = Some(
                crate::domain::wish::entities::WishNegotiationStatus {
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
                }
            );
            return Ok(out);
        }

        // 4) 双方都 ACCEPT → 进 CREATED,wish_cost 同步为 final_cost
        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CREATED'::wish_status_enum, wish_cost = $2, updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .bind(final_cost)
        .fetch_one(db)
        .await?;

        let _ = EventPublisher::publish(
            db,
            EventType::WishAgreementConfirmed,
            WishAgreementConfirmedPayload {
                wish_id,
                requester_id,
                fulfiller_id,
                group_id: existing.group_id,
                final_cost: existing.final_cost.unwrap_or(existing.wish_cost),
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

    /// 拒绝或关闭心愿
    /// action: "REJECT"(仅协商阶段使用,语义"我拒绝这个提议") | "CLOSE"(任意非终态使用,语义"主动关闭心愿")
    pub async fn reject_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishRejectInput,
        action: &str,  // P3-4: 区分 REJECT(仅协商中) 和 CLOSE(任意非终态)
    ) -> Result<WishRecord, CustomError> {
        // P3-4: REJECT 仅允许在 NEGOTIATING 状态,CLOSE 允许任意非终态
        let allowed_statuses: &[WishStatus] = if action == "REJECT" {
            &[WishStatus::Negotiating]
        } else {
            // CLOSE: 任意非终态都可以
            return Self::close_wish_internal(db, user_id, wish_id, input).await;
        };

        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        if !allowed_statuses.contains(&existing.status) {
            return Err(CustomError::BadRequest(
                "协商拒绝仅在 NEGOTIATING 状态可用,关闭请用 /close 接口".into(),
            ));
        }

        // P3-3: 操作者角色快照
        let role_snapshot_close = if user_id == existing.requester_id.unwrap_or(existing.created_by) {
            "REQUESTER"
        } else {
            "FULFILLER"
        };

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, remark) VALUES ($1, $2, $3, $4, $5::wish_negotiation_action_enum, $6)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(role_snapshot_close)
        .bind(action)
        .bind(&input.reason)
        .execute(db)
        .await?;

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLOSED'::wish_status_enum, closed_at = NOW(), updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        let _ = EventPublisher::publish(
            db,
            EventType::WishClosed,
            WishClosedPayload {
                wish_id,
                operator_id: user_id,
                group_id: existing.group_id,
                reason: input.reason.clone(),
                unfrozen_if_any: false,
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

    /// /close 接口专用 - 任意非终态都可关闭,CLAIMED 时自动解冻
    async fn close_wish_internal(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        input: &WishRejectInput,
    ) -> Result<WishRecord, CustomError> {
        let existing = sqlx::query_as::<_, WishRecord>(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours FROM wishes WHERE wish_id = $1"
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

        if existing.status.is_terminal() {
            return Err(CustomError::BadRequest("当前状态不允许关闭".into()));
        }

        // CLAIMED 状态下:积分已被冻结,关闭时必须解冻,否则用户积分流失
        let was_claimed = existing.status == WishStatus::Claimed;
        if was_claimed {
            Self::unfreeze_wish_points(db, wish_id, existing.group_id, existing.requester_id.unwrap_or(existing.created_by), "wish_close").await?;
        }

        let role_snapshot_close = if user_id == existing.requester_id.unwrap_or(existing.created_by) {
            "REQUESTER"
        } else {
            "FULFILLER"
        };

        sqlx::query(
            "INSERT INTO wish_negotiations (wish_id, group_id, operator_id, operator_role_snapshot, action, remark) VALUES ($1, $2, $3, $4, 'CLOSE'::wish_negotiation_action_enum, $5)"
        )
        .bind(wish_id)
        .bind(existing.group_id)
        .bind(user_id)
        .bind(role_snapshot_close)
        .bind(&input.reason)
        .execute(db)
        .await?;

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLOSED'::wish_status_enum, closed_at = NOW(), updated_at = NOW() WHERE wish_id = $1 \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        let _ = EventPublisher::publish(
            db,
            EventType::WishClosed,
            WishClosedPayload {
                wish_id,
                operator_id: user_id,
                group_id: existing.group_id,
                reason: input.reason.clone(),
                unfrozen_if_any: was_claimed,
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

    /// 解冻心愿冻结积分 - 由 wish_expire 和 reject_wish(CLAIMED 时)共用
    /// 通过 idempotency_key 保证同一心愿只解冻一次
    pub async fn unfreeze_wish_points(
        db: &PgPool,
        wish_id: i64,
        group_id: i64,
        requester_id: i64,
        idempotency_key: &str,
    ) -> Result<i64, CustomError> {
        let frozen_amount: i64 = sqlx::query_scalar::<_, i64>(
            r#"SELECT COALESCE(SUM(CASE WHEN type='FREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint - SUM(CASE WHEN type='UNFREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint, 0::bigint) FROM love_point_transactions WHERE user_id=$1 AND group_id=$2 AND biz_id=$3 AND biz_type = 'wish'"#
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(wish_id)
        .fetch_optional(db)
        .await?
        .unwrap_or(0);

        if frozen_amount <= 0 {
            return Ok(0);
        }

        // 幂等检查:同一 idempotency_key 已写入则跳过
        let already: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM love_point_transactions WHERE idempotency_key = $1 LIMIT 1"
        )
        .bind(idempotency_key)
        .fetch_optional(db)
        .await?;
        if already.is_some() {
            return Ok(0);
        }

        let row = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(SUM(CASE WHEN type IN ('EARN'::love_point_tx_type_enum) THEN amount ELSE 0 END)::bigint, 0::bigint), COALESCE(SUM(CASE WHEN type='FREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint - SUM(CASE WHEN type='UNFREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint, 0::bigint) FROM love_point_transactions WHERE user_id=$1 AND group_id=$2"
        )
        .bind(requester_id)
        .bind(group_id)
        .fetch_one(db)
        .await?;
        let (available_before, frozen_before) = row;

        sqlx::query(
            r#"INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at)
               VALUES ($1, $2, 'UNFREEZE'::love_point_tx_type_enum, $3, $4, $4+$3, $5, 0, 'wish', $6, $7, NOW())"#
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(frozen_amount)
        .bind(available_before)
        .bind(frozen_before)
        .bind(wish_id)
        .bind(idempotency_key)
        .execute(db)
        .await?;

        Ok(frozen_amount)
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

        // 心愿商城模型:心愿的「实现」由履约方线下满足,
        // 兑换心愿(出积分)的只能是创建方(requester_id),不能是履约方乱兑
        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        if user_id != requester_id {
            return Err(CustomError::Forbidden(
                "只有心愿创建方可以兑换".into(),
            ));
        }

        let points_cost = existing.final_cost.unwrap_or(existing.wish_cost);

        let mut tx = db.begin().await?;

        // SELECT FOR UPDATE 锁住 users 行,确保后续 UPDATE 看到的余额是最新值
        let current_points: i32 = sqlx::query_scalar(
            "SELECT love_point FROM users WHERE user_id = $1 FOR UPDATE"
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

        if current_points < points_cost {
            return Err(CustomError::BadRequest("爱心积分不足".into()));
        }

        // 冻结积分：可用减少,冻结增加(冻结字段由事务内计算,不再依赖外部读)
        let frozen_before = 0i32;
        let available_after = current_points - points_cost;
        let frozen_after = points_cost;

        sqlx::query("UPDATE users SET love_point = $2 WHERE user_id = $1")
            .bind(user_id)
            .bind(available_after)
            .execute(&mut *tx)
            .await?;

        // 写积分流水(冻结) - biz_type 统一为 'wish'
        sqlx::query(
            "INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, trace_id, created_at) \
             VALUES ($1, $2, 'FREEZE'::love_point_tx_type_enum, $3, $4, $5, $6, $7, 'wish', $8, '', NOW())"
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

        // 更新心愿状态为 CLAIMED(条件 UPDATE 防止 TOCTOU 重复 select)
        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'CLAIMED'::wish_status_enum, selected_by = $2, selected_at = NOW(), claimed_by = $2, fulfillment_due_at = $3, updated_at = NOW() \
             WHERE wish_id = $1 AND status = 'CREATED'::wish_status_enum AND claimed_by IS NULL \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, fulfillment_due_at, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(fulfillment_due_at)
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
        )
        .await;

        Ok(rec)
    }

    /// 接单人确认履约完成 — 把心愿从 CLAIMED 推到 FINISHED
    ///
    /// 业务流:履约人提交打卡后(状态保持 CLAIMED),由接单人(requester_id)
    /// 在 wishDetail 页面点「确认完成」,才推进到 FINISHED。
    ///
    /// 积分在 select_wish 阶段已 FREEZE,FINISHED 不再扣减 —— 走 reject_wish 的
    /// UNFREEZE 路径会让积分回到余额;FINISHED 不走 UNFREEZE,相当于「正式扣下」。
    pub async fn confirm_wish_completion(
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

        // 权限:只有接单人(requester_id)能确认完成
        let requester_id = existing.requester_id.unwrap_or(existing.created_by);
        if user_id != requester_id {
            return Err(CustomError::Forbidden(
                "只有接单人可以确认完成".into(),
            ));
        }

        // 状态:必须是 CLAIMED
        if existing.status != WishStatus::Claimed {
            return Err(CustomError::BadRequest(
                "只有履约中的心愿可以确认完成".into(),
            ));
        }

        // 必须有打卡记录(履约人必须先提交打卡)
        let feedback_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM wish_feedbacks WHERE wish_id = $1)"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;
        if !feedback_exists {
            return Err(CustomError::BadRequest(
                "履约人尚未提交打卡,无法确认完成".into(),
            ));
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status = 'FINISHED'::wish_status_enum, updated_at = NOW() WHERE wish_id = $1 AND status = 'CLAIMED'::wish_status_enum \
             RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, claimed_by, claimed_at, claim_cost, created_at, updated_at, \
             requester_id, fulfiller_id, initial_cost, final_cost, fulfillment_deadline_hours"
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        let deducted_amount = existing.final_cost.unwrap_or(existing.wish_cost);
        let _ = EventPublisher::publish(
            db,
            EventType::WishFinished,
            WishFinishedPayload {
                wish_id,
                requester_id,
                fulfiller_id: existing.fulfiller_id.unwrap_or(0),
                group_id: existing.group_id,
                deducted_amount,
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
}
