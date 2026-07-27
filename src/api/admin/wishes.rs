use ntex::web::{
    types::{Json, Path, Query, State},
    Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

use crate::config::AppState;
use crate::domain::event::{EventType, WishQualityRewardedPayload};
use crate::errors::CustomError;
use crate::infrastructure::event::publisher::EventPublisher;
use crate::middlewares::admin_auth::{require_admin_role, AdminRole, AdminToken};
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use crate::utils::response::ApiResponse;

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AdminWishQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<String>,
    pub quality_review_status: Option<String>,
    pub quality_level: Option<String>,
    pub group_id: Option<i64>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub pending_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AdminWishCursor {
    created_at: chrono::DateTime<chrono::Utc>,
    wish_id: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminWishListItem {
    pub wish_id: i64,
    pub wish_name: String,
    pub status: String,
    pub group_id: i64,
    pub group_name: Option<String>,
    pub requester_id: Option<i64>,
    pub requester_nickname: Option<String>,
    pub fulfiller_id: Option<i64>,
    pub fulfiller_nickname: Option<String>,
    pub final_cost: i32,
    pub quality_review_status: String,
    pub quality_level: String,
    pub diamond_reward: i32,
    pub feedback_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminWishFeedback {
    pub feedback_id: i64,
    pub user_id: i64,
    pub role: Option<String>,
    pub content: Option<String>,
    pub images: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminWishNegotiation {
    pub id: i64,
    pub operator_id: i64,
    pub role: Option<String>,
    pub action: String,
    pub cost: Option<i32>,
    pub deadline_hours: Option<i32>,
    pub remark: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminWishDetail {
    #[serde(flatten)]
    pub summary: AdminWishListItem,
    pub initial_cost: Option<i32>,
    pub fulfillment_deadline_hours: Option<i32>,
    pub fulfillment_due_at: Option<chrono::DateTime<chrono::Utc>>,
    pub fulfilled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expired_at: Option<chrono::DateTime<chrono::Utc>>,
    pub auto_completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub quality_reviewer_id: Option<i64>,
    pub quality_remark: Option<String>,
    pub feedbacks: Vec<AdminWishFeedback>,
    pub negotiations: Vec<AdminWishNegotiation>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminWishQualityInput {
    pub quality_level: String,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminWishQualityResponse {
    pub wish_id: i64,
    pub quality_level: String,
    pub diamond_reward: i32,
    pub quality_review_status: String,
    pub already_reviewed: bool,
}

const WISH_LIST_SELECT: &str = r#"
    SELECT w.wish_id, w.wish_name, w.status::text AS status, w.group_id, g.group_name,
           w.requester_id, requester.nick_name AS requester_nickname,
           w.fulfiller_id, fulfiller.nick_name AS fulfiller_nickname,
           COALESCE(w.final_cost, w.wish_cost) AS final_cost,
           COALESCE(w.quality_review_status::text, 'NONE') AS quality_review_status,
           COALESCE(w.quality_level::text, 'NONE') AS quality_level,
           COALESCE(w.diamond_reward, 0) AS diamond_reward,
           (SELECT COUNT(*) FROM wish_feedbacks wf WHERE wf.wish_id = w.wish_id) AS feedback_count,
           w.created_at, w.finished_at
    FROM wishes w
    LEFT JOIN association_groups g ON g.group_id = w.group_id
    LEFT JOIN users requester ON requester.user_id = w.requester_id
    LEFT JOIN users fulfiller ON fulfiller.user_id = w.fulfiller_id
"#;

fn row_to_list_item(row: &sqlx::postgres::PgRow) -> AdminWishListItem {
    AdminWishListItem {
        wish_id: row.get("wish_id"),
        wish_name: row.get("wish_name"),
        status: row.get("status"),
        group_id: row.get("group_id"),
        group_name: row.try_get("group_name").ok().flatten(),
        requester_id: row.try_get("requester_id").ok().flatten(),
        requester_nickname: row.try_get("requester_nickname").ok().flatten(),
        fulfiller_id: row.try_get("fulfiller_id").ok().flatten(),
        fulfiller_nickname: row.try_get("fulfiller_nickname").ok().flatten(),
        final_cost: row.get("final_cost"),
        quality_review_status: row.get("quality_review_status"),
        quality_level: row.get("quality_level"),
        diamond_reward: row.get("diamond_reward"),
        feedback_count: row.get("feedback_count"),
        created_at: row.get("created_at"),
        finished_at: row.try_get("finished_at").ok().flatten(),
    }
}

#[utoipa::path(
    get,
    path = "/api/admin/wishes",
    tag = "后台管理",
    params(AdminWishQuery),
    responses((status = 200, body = CursorPage<AdminWishListItem>)),
    security(("bearer_auth" = []))
)]
pub async fn list_admin_wishes(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    query: Query<AdminWishQuery>,
) -> Result<impl Responder, CustomError> {
    require_admin_role(&admin, &[AdminRole::Ops])?;
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let cursor = query
        .cursor
        .as_deref()
        .and_then(decode_cursor::<AdminWishCursor>);
    let (cursor_at, cursor_id) = cursor
        .map(|cursor| (Some(cursor.created_at), Some(cursor.wish_id)))
        .unwrap_or((None, None));
    let sql = format!(
        r#"{WISH_LIST_SELECT}
        WHERE ($1::text IS NULL OR w.status::text = $1)
          AND ($2::text IS NULL OR w.quality_review_status::text = $2)
          AND ($3::text IS NULL OR w.quality_level::text = $3)
          AND ($4::bigint IS NULL OR w.group_id = $4)
          AND ($5::timestamptz IS NULL OR w.created_at >= $5)
          AND ($6::timestamptz IS NULL OR w.created_at <= $6)
          AND ($7::boolean = FALSE OR
               (w.status = 'FINISHED'::wish_status_enum AND
                w.quality_review_status = 'NONE'::wish_quality_status_enum))
          AND ($8::timestamptz IS NULL OR (w.created_at, w.wish_id) < ($8, $9))
        ORDER BY w.created_at DESC, w.wish_id DESC
        LIMIT $10"#
    );
    let rows = sqlx::query(&sql)
        .bind(query.status.as_deref())
        .bind(query.quality_review_status.as_deref())
        .bind(query.quality_level.as_deref())
        .bind(query.group_id)
        .bind(query.start_date.as_ref())
        .bind(query.end_date.as_ref())
        .bind(query.pending_only)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit + 1)
        .fetch_all(&state.db_pool)
        .await?;
    let has_more = rows.len() as i64 > limit;
    let items: Vec<AdminWishListItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| row_to_list_item(&row))
        .collect();
    let next_cursor = if has_more {
        items.last().map(|item| {
            encode_cursor(&AdminWishCursor {
                created_at: item.created_at,
                wish_id: item.wish_id,
            })
        })
    } else {
        None
    };
    Ok(ApiResponse::success(CursorPage {
        items,
        next_cursor,
        has_more,
        total: None,
    }))
}

#[utoipa::path(
    get,
    path = "/api/admin/wishes/{wish_id}",
    tag = "后台管理",
    params(("wish_id" = i64, Path)),
    responses((status = 200, body = AdminWishDetail), (status = 404)),
    security(("bearer_auth" = []))
)]
pub async fn get_admin_wish(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
) -> Result<impl Responder, CustomError> {
    require_admin_role(&admin, &[AdminRole::Ops])?;
    let wish_id = path.into_inner();
    let sql = format!(r#"{WISH_LIST_SELECT} WHERE w.wish_id = $1"#);
    let row = sqlx::query(&sql)
        .bind(wish_id)
        .fetch_optional(&state.db_pool)
        .await?
        .ok_or_else(|| CustomError::wish_not_found("心愿不存在"))?;
    let summary = row_to_list_item(&row);
    let extra = sqlx::query(
        "SELECT initial_cost, fulfillment_deadline_hours, fulfillment_due_at, fulfilled_at, \
                expired_at, auto_completed_at, quality_reviewer_id, quality_remark \
         FROM wishes WHERE wish_id = $1",
    )
    .bind(wish_id)
    .fetch_one(&state.db_pool)
    .await?;
    let feedback_rows = sqlx::query(
        "SELECT feedback_id, user_id, role_snapshot, content, images, created_at, updated_at \
         FROM wish_feedbacks WHERE wish_id = $1 ORDER BY created_at ASC",
    )
    .bind(wish_id)
    .fetch_all(&state.db_pool)
    .await?;
    let negotiation_rows = sqlx::query(
        "SELECT id, operator_id, operator_role_snapshot, action::text AS action, cost, \
                deadline_hours, remark, created_at \
         FROM wish_negotiations WHERE wish_id = $1 ORDER BY created_at ASC, id ASC",
    )
    .bind(wish_id)
    .fetch_all(&state.db_pool)
    .await?;
    let feedbacks = feedback_rows
        .into_iter()
        .map(|row| AdminWishFeedback {
            feedback_id: row.get("feedback_id"),
            user_id: row.get("user_id"),
            role: row.try_get("role_snapshot").ok().flatten(),
            content: row.try_get("content").ok().flatten(),
            images: row.try_get("images").ok().flatten(),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();
    let negotiations = negotiation_rows
        .into_iter()
        .map(|row| AdminWishNegotiation {
            id: row.get("id"),
            operator_id: row.get("operator_id"),
            role: row.try_get("operator_role_snapshot").ok().flatten(),
            action: row.get("action"),
            cost: row.try_get("cost").ok().flatten(),
            deadline_hours: row.try_get("deadline_hours").ok().flatten(),
            remark: row.try_get("remark").ok().flatten(),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(ApiResponse::success(AdminWishDetail {
        summary,
        initial_cost: extra.try_get("initial_cost").ok().flatten(),
        fulfillment_deadline_hours: extra.try_get("fulfillment_deadline_hours").ok().flatten(),
        fulfillment_due_at: extra.try_get("fulfillment_due_at").ok().flatten(),
        fulfilled_at: extra.try_get("fulfilled_at").ok().flatten(),
        expired_at: extra.try_get("expired_at").ok().flatten(),
        auto_completed_at: extra.try_get("auto_completed_at").ok().flatten(),
        quality_reviewer_id: extra.try_get("quality_reviewer_id").ok().flatten(),
        quality_remark: extra.try_get("quality_remark").ok().flatten(),
        feedbacks,
        negotiations,
    }))
}

#[utoipa::path(
    post,
    path = "/api/admin/wishes/{wish_id}/quality-reward",
    tag = "后台管理",
    params(("wish_id" = i64, Path)),
    request_body = AdminWishQualityInput,
    responses((status = 200, body = AdminWishQualityResponse), (status = 404)),
    security(("bearer_auth" = []))
)]
pub async fn review_wish_quality(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
    body: Json<AdminWishQualityInput>,
) -> Result<impl Responder, CustomError> {
    require_admin_role(&admin, &[AdminRole::Ops])?;
    let wish_id = path.into_inner();
    let input = body.into_inner();
    let diamond_amount = match input.quality_level.as_str() {
        "NONE" => 0,
        "NORMAL" => 5,
        "GOOD" => 10,
        "EXCELLENT" => 20,
        _ => return Err(CustomError::BadRequest("无效的质量等级".into())),
    };
    let mut tx = state.db_pool.begin().await?;
    let row = sqlx::query(
        "SELECT status::text AS status, group_id, requester_id, \
                quality_review_status::text AS review_status, \
                quality_level::text AS quality_level, COALESCE(diamond_reward, 0) AS diamond_reward \
         FROM wishes WHERE wish_id = $1 FOR UPDATE",
    )
    .bind(wish_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| CustomError::wish_not_found("心愿不存在"))?;
    let status: String = row.get("status");
    if status != "FINISHED" {
        return Err(CustomError::BadRequest("只有已完成心愿可以质量评价".into()));
    }
    let review_status: String = row.get("review_status");
    if review_status == "REVIEWED" {
        tx.commit().await?;
        return Ok(ApiResponse::success(AdminWishQualityResponse {
            wish_id,
            quality_level: row.get("quality_level"),
            diamond_reward: row.get("diamond_reward"),
            quality_review_status: review_status,
            already_reviewed: true,
        }));
    }
    let group_id: i64 = row.get("group_id");
    let requester_id: Option<i64> = row.try_get("requester_id").ok().flatten();
    sqlx::query(
        "UPDATE wishes SET quality_review_status = 'REVIEWED'::wish_quality_status_enum, \
                quality_level = $2::wish_quality_level_enum, quality_reviewer_id = $3, \
                quality_remark = $4, diamond_reward = $5, updated_at = NOW() \
         WHERE wish_id = $1",
    )
    .bind(wish_id)
    .bind(&input.quality_level)
    .bind(admin.user_id)
    .bind(&input.remark)
    .bind(diamond_amount)
    .execute(&mut *tx)
    .await?;

    if diamond_amount > 0 {
        let before: i32 = sqlx::query_scalar(
            "SELECT diamond FROM association_groups WHERE group_id = $1 FOR UPDATE",
        )
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;
        let after = before
            .checked_add(diamond_amount)
            .ok_or_else(|| CustomError::internal("组钻石溢出"))?;
        sqlx::query(
            "UPDATE association_groups SET diamond = $2, updated_at = NOW() WHERE group_id = $1",
        )
        .bind(group_id)
        .bind(after)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO diamond_transactions \
                 (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, \
                  idempotency_key, created_at) \
             VALUES ($1, 'EARN'::diamond_tx_type_enum, $2, $3, $4, \
                     'WISH_QUALITY_REWARD', $5, $6, NOW())",
        )
        .bind(group_id)
        .bind(diamond_amount)
        .bind(before)
        .bind(after)
        .bind(wish_id)
        .bind(format!("wish_quality_reward_{wish_id}"))
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO audit_logs \
             (operator_id, operator_type, action_type, target_type, target_id, detail, created_at) \
         VALUES ($1, 'ADMIN', 'WISH_QUALITY_REWARD', 'WISH', $2, $3, NOW())",
    )
    .bind(admin.user_id)
    .bind(wish_id)
    .bind(serde_json::json!({
        "qualityLevel": input.quality_level,
        "diamondReward": diamond_amount,
        "remark": input.remark,
    }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    crate::api::wishes::broadcast::push_wish_quality_review_notice(
        &state.db_pool,
        wish_id,
        group_id,
        &input.quality_level,
        diamond_amount,
    )
    .await;
    if diamond_amount > 0 {
        crate::api::sign_in::broadcast::push_group_diamond_change_notice(
            &state.db_pool,
            group_id,
            requester_id.unwrap_or(admin.user_id),
            "wish_quality_reward",
            0,
        )
        .await;
    }
    let _ = EventPublisher::publish(
        &state.db_pool,
        EventType::WishQualityRewarded,
        WishQualityRewardedPayload {
            wish_id,
            group_id,
            quality_level: input.quality_level.clone(),
            diamond_reward: diamond_amount,
            reviewer_id: admin.user_id,
            trace_id: None,
        },
        requester_id,
        Some(group_id),
        Some("wish"),
        Some(wish_id),
    )
    .await;

    Ok(ApiResponse::success(AdminWishQualityResponse {
        wish_id,
        quality_level: input.quality_level,
        diamond_reward: diamond_amount,
        quality_review_status: "REVIEWED".to_string(),
        already_reviewed: false,
    }))
}
