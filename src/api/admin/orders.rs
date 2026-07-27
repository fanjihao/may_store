use ntex::web::{
    types::{Json, Path, Query, State},
    Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::admin_auth::{require_admin_role, AdminRole, AdminToken};
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use crate::utils::response::ApiResponse;

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AdminOrderQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<String>,
    pub risk_status: Option<String>,
    pub grant_status: Option<String>,
    pub order_type: Option<String>,
    pub group_id: Option<i64>,
    pub user_id: Option<i64>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub pending_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AdminOrderCursor {
    created_at: chrono::DateTime<chrono::Utc>,
    order_id: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminOrderListItem {
    pub order_id: i64,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub order_type: String,
    pub user_id: i64,
    pub user_nickname: Option<String>,
    pub assignee_id: Option<i64>,
    pub assignee_nickname: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub risk_status: String,
    pub risk_detail: Option<serde_json::Value>,
    pub points_reward: i32,
    pub group_exp_reward: i32,
    pub point_grant_status: String,
    pub exp_grant_status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminOrderItem {
    pub id: i64,
    pub food_id: i64,
    pub food_name: Option<String>,
    pub quantity: i32,
    pub price: Option<f64>,
    pub snapshot: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminOrderHistoryItem {
    pub id: i64,
    pub from_status: Option<String>,
    pub to_status: String,
    pub changed_by: Option<i64>,
    pub remark: Option<String>,
    pub changed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminOrderDetail {
    #[serde(flatten)]
    pub summary: AdminOrderListItem,
    pub content: Option<String>,
    pub remark: Option<String>,
    pub goal_time: Option<chrono::DateTime<chrono::Utc>>,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub cancel_reason: Option<String>,
    pub reject_reason: Option<String>,
    pub items: Vec<AdminOrderItem>,
    pub history: Vec<AdminOrderHistoryItem>,
    pub point_transactions: Vec<serde_json::Value>,
    pub exp_transactions: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminOrderReviewInput {
    pub approve_point: bool,
    pub approve_exp: bool,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminOrderReviewResponse {
    pub order_id: i64,
    pub point_grant_status: String,
    pub exp_grant_status: String,
    pub point_amount_granted: i32,
    pub exp_amount_granted: i64,
    pub already_reviewed: bool,
}

fn row_to_list_item(row: &sqlx::postgres::PgRow) -> AdminOrderListItem {
    AdminOrderListItem {
        order_id: row.get("order_id"),
        group_id: row.try_get("group_id").ok().flatten(),
        group_name: row.try_get("group_name").ok().flatten(),
        order_type: row.get("order_type"),
        user_id: row.get("user_id"),
        user_nickname: row.try_get("user_nickname").ok().flatten(),
        assignee_id: row.try_get("assignee_id").ok().flatten(),
        assignee_nickname: row.try_get("assignee_nickname").ok().flatten(),
        title: row.try_get("title").ok().flatten(),
        status: row.get("status"),
        risk_status: row.get("risk_status"),
        risk_detail: row.try_get("risk_detail").ok().flatten(),
        points_reward: row.get("points_reward"),
        group_exp_reward: row.get("group_exp_reward"),
        point_grant_status: row.get("point_grant_status"),
        exp_grant_status: row.get("exp_grant_status"),
        created_at: row.get("created_at"),
    }
}

const ORDER_LIST_SELECT: &str = r#"
    SELECT o.order_id, o.group_id, g.group_name,
           o.type::text AS order_type, o.user_id, creator.nick_name AS user_nickname,
           o.assignee_id, assignee.nick_name AS assignee_nickname, o.title,
           o.status::text AS status, o.risk_status::text AS risk_status, o.risk_detail,
           COALESCE(point_actual.actual_reward, o.points_reward) AS points_reward,
           COALESCE(exp_actual.actual_reward, o.group_exp_reward) AS group_exp_reward,
           CASE
             WHEN o.point_grant_status = 'NONE'::point_grant_status_enum
                  AND point_actual.actual_reward IS NOT NULL THEN 'GRANTED'
             ELSE o.point_grant_status::text
           END AS point_grant_status,
           CASE
             WHEN o.exp_grant_status = 'NONE'::exp_grant_status_enum
                  AND exp_actual.actual_reward IS NOT NULL THEN 'GRANTED'
             ELSE o.exp_grant_status::text
           END AS exp_grant_status,
           o.created_at
    FROM orders o
    LEFT JOIN association_groups g ON g.group_id = o.group_id
    LEFT JOIN users creator ON creator.user_id = o.user_id
    LEFT JOIN users assignee ON assignee.user_id = o.assignee_id
    LEFT JOIN LATERAL (
      SELECT SUM(amount)::INT AS actual_reward
      FROM love_point_transactions
      WHERE biz_id = o.order_id
        AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW')
        AND type = 'EARN'::love_point_tx_type_enum
    ) point_actual ON TRUE
    LEFT JOIN LATERAL (
      SELECT SUM(amount)::INT AS actual_reward
      FROM group_exp_transactions
      WHERE biz_id = o.order_id
        AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW')
        AND type = 'EARN'::group_exp_tx_type_enum
    ) exp_actual ON TRUE
"#;

#[utoipa::path(
    get,
    path = "/api/admin/orders",
    tag = "后台管理",
    params(AdminOrderQuery),
    responses((status = 200, body = CursorPage<AdminOrderListItem>)),
    security(("bearer_auth" = []))
)]
pub async fn list_admin_orders(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    query: Query<AdminOrderQuery>,
) -> Result<impl Responder, CustomError> {
    require_admin_role(&admin, &[AdminRole::RiskReviewer])?;
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let cursor = query
        .cursor
        .as_deref()
        .and_then(decode_cursor::<AdminOrderCursor>);
    let (cursor_at, cursor_id) = cursor
        .map(|cursor| (Some(cursor.created_at), Some(cursor.order_id)))
        .unwrap_or((None, None));

    let sql = format!(
        r#"{ORDER_LIST_SELECT}
        WHERE ($1::text IS NULL OR o.status::text = $1)
          AND ($2::text IS NULL OR o.risk_status::text = $2)
          AND ($3::text IS NULL OR
               (CASE
                  WHEN o.point_grant_status = 'NONE'::point_grant_status_enum
                       AND point_actual.actual_reward IS NOT NULL THEN 'GRANTED'
                  ELSE o.point_grant_status::text
                END) = $3 OR
               (CASE
                  WHEN o.exp_grant_status = 'NONE'::exp_grant_status_enum
                       AND exp_actual.actual_reward IS NOT NULL THEN 'GRANTED'
                  ELSE o.exp_grant_status::text
                END) = $3)
          AND ($4::text IS NULL OR o.type::text = $4)
          AND ($5::bigint IS NULL OR o.group_id = $5)
          AND ($6::bigint IS NULL OR o.user_id = $6)
          AND ($7::timestamptz IS NULL OR o.created_at >= $7)
          AND ($8::timestamptz IS NULL OR o.created_at <= $8)
          AND ($9::boolean = FALSE OR
               o.point_grant_status = 'PENDING_REVIEW'::point_grant_status_enum OR
               o.exp_grant_status = 'PENDING_REVIEW'::exp_grant_status_enum)
          AND ($10::timestamptz IS NULL OR (o.created_at, o.order_id) < ($10, $11))
        ORDER BY o.created_at DESC, o.order_id DESC
        LIMIT $12"#
    );

    let rows = sqlx::query(&sql)
        .bind(query.status.as_deref())
        .bind(query.risk_status.as_deref())
        .bind(query.grant_status.as_deref())
        .bind(query.order_type.as_deref())
        .bind(query.group_id)
        .bind(query.user_id)
        .bind(query.start_date)
        .bind(query.end_date)
        .bind(query.pending_only)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit + 1)
        .fetch_all(&state.db_pool)
        .await?;

    let has_more = rows.len() as i64 > limit;
    let mut items: Vec<AdminOrderListItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| row_to_list_item(&row))
        .collect();
    let next_cursor = if has_more {
        items.last().map(|item| {
            encode_cursor(&AdminOrderCursor {
                created_at: item.created_at,
                order_id: item.order_id,
            })
        })
    } else {
        None
    };
    if !has_more {
        items.shrink_to_fit();
    }
    Ok(ApiResponse::success(CursorPage {
        items,
        next_cursor,
        has_more,
        total: None,
    }))
}

#[utoipa::path(
    get,
    path = "/api/admin/orders/{order_id}",
    tag = "后台管理",
    params(("order_id" = i64, Path)),
    responses((status = 200, body = AdminOrderDetail), (status = 404)),
    security(("bearer_auth" = []))
)]
pub async fn get_admin_order(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
) -> Result<impl Responder, CustomError> {
    require_admin_role(&admin, &[AdminRole::RiskReviewer])?;
    let order_id = path.into_inner();
    let sql = format!(
        r#"{ORDER_LIST_SELECT}
        WHERE o.order_id = $1"#
    );
    let row = sqlx::query(&sql)
        .bind(order_id)
        .fetch_optional(&state.db_pool)
        .await?
        .ok_or_else(|| CustomError::order_not_found("订单不存在"))?;
    let summary = row_to_list_item(&row);

    let extra = sqlx::query(
        "SELECT content, remark, goal_time, deadline, accepted_at, completed_at, confirmed_at, \
                cancel_reason, reject_reason FROM orders WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&state.db_pool)
    .await?;
    let item_rows = sqlx::query(
        "SELECT oi.id, oi.food_id, f.food_name, oi.quantity, oi.price, oi.snapshot_json \
         FROM order_items oi LEFT JOIN foods f ON f.food_id = oi.food_id \
         WHERE oi.order_id = $1 ORDER BY oi.id",
    )
    .bind(order_id)
    .fetch_all(&state.db_pool)
    .await?;
    let history_rows = sqlx::query(
        "SELECT id, from_status::text AS from_status, to_status::text AS to_status, \
                changed_by, remark, changed_at FROM order_status_history \
         WHERE order_id = $1 ORDER BY changed_at ASC, id ASC",
    )
    .bind(order_id)
    .fetch_all(&state.db_pool)
    .await?;
    let point_rows = sqlx::query(
        "SELECT id, type::text AS type, amount, available_before, available_after, \
                frozen_before, frozen_after, biz_type, created_at \
         FROM love_point_transactions WHERE biz_id = $1 \
           AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW') \
         ORDER BY created_at ASC",
    )
    .bind(order_id)
    .fetch_all(&state.db_pool)
    .await?;
    let exp_rows = sqlx::query(
        "SELECT id, type::text AS type, amount, exp_before, exp_after, \
                level_before, level_after, biz_type, created_at \
         FROM group_exp_transactions WHERE biz_id = $1 \
           AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW') \
         ORDER BY created_at ASC",
    )
    .bind(order_id)
    .fetch_all(&state.db_pool)
    .await?;

    let items = item_rows
        .into_iter()
        .map(|row| AdminOrderItem {
            id: row.get("id"),
            food_id: row.get("food_id"),
            food_name: row.try_get("food_name").ok().flatten(),
            quantity: row.get("quantity"),
            price: row.try_get("price").ok().flatten(),
            snapshot: row.try_get("snapshot_json").ok().flatten(),
        })
        .collect();
    let history = history_rows
        .into_iter()
        .map(|row| AdminOrderHistoryItem {
            id: row.get("id"),
            from_status: row.try_get("from_status").ok().flatten(),
            to_status: row.get("to_status"),
            changed_by: row.try_get("changed_by").ok().flatten(),
            remark: row.try_get("remark").ok().flatten(),
            changed_at: row.get("changed_at"),
        })
        .collect();
    let point_transactions = point_rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "type": row.get::<String, _>("type"),
                "amount": row.get::<i64, _>("amount"),
                "before": row.get::<i64, _>("available_before"),
                "after": row.get::<i64, _>("available_after"),
                "frozenBefore": row.get::<i64, _>("frozen_before"),
                "frozenAfter": row.get::<i64, _>("frozen_after"),
                "bizType": row.get::<String, _>("biz_type"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();
    let exp_transactions = exp_rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<i64, _>("id"),
                "type": row.get::<String, _>("type"),
                "amount": row.get::<i64, _>("amount"),
                "before": row.get::<i64, _>("exp_before"),
                "after": row.get::<i64, _>("exp_after"),
                "levelBefore": row.get::<i32, _>("level_before"),
                "levelAfter": row.get::<i32, _>("level_after"),
                "bizType": row.get::<String, _>("biz_type"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    Ok(ApiResponse::success(AdminOrderDetail {
        summary,
        content: extra.try_get("content").ok().flatten(),
        remark: extra.try_get("remark").ok().flatten(),
        goal_time: extra.try_get("goal_time").ok().flatten(),
        deadline: extra.try_get("deadline").ok().flatten(),
        accepted_at: extra.try_get("accepted_at").ok().flatten(),
        completed_at: extra.try_get("completed_at").ok().flatten(),
        confirmed_at: extra.try_get("confirmed_at").ok().flatten(),
        cancel_reason: extra.try_get("cancel_reason").ok().flatten(),
        reject_reason: extra.try_get("reject_reason").ok().flatten(),
        items,
        history,
        point_transactions,
        exp_transactions,
    }))
}

async fn grant_point_reward(
    tx: &mut Transaction<'_, Postgres>,
    order_id: i64,
    group_id: i64,
    user_id: i64,
    reward: i32,
) -> Result<i32, CustomError> {
    if reward <= 0 {
        return Ok(0);
    }
    let already_granted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM love_point_transactions \
         WHERE biz_id = $1 AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW') \
           AND type = 'EARN'::love_point_tx_type_enum)",
    )
    .bind(order_id)
    .fetch_one(&mut **tx)
    .await?;
    if already_granted {
        return Ok(0);
    }
    let before: i32 =
        sqlx::query_scalar("SELECT love_point FROM users WHERE user_id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?;
    let after = before
        .checked_add(reward)
        .ok_or_else(|| CustomError::internal("爱心积分溢出"))?;
    let frozen: i64 = sqlx::query_scalar(
        "SELECT COALESCE(frozen_love_point, 0) FROM user_group_points \
         WHERE user_id = $1 AND group_id = $2 FOR UPDATE",
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_optional(&mut **tx)
    .await?
    .unwrap_or(0);
    sqlx::query("UPDATE users SET love_point = $2 WHERE user_id = $1")
        .bind(user_id)
        .bind(after)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO user_group_points \
             (user_id, group_id, available_love_point, love_point, frozen_love_point, updated_at) \
         VALUES ($1, $2, $3, $3, $4, NOW()) \
         ON CONFLICT (user_id, group_id) DO UPDATE \
         SET available_love_point = $3, love_point = $3, updated_at = NOW()",
    )
    .bind(user_id)
    .bind(group_id)
    .bind(i64::from(after))
    .bind(frozen)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO love_point_transactions \
             (user_id, group_id, type, amount, available_before, available_after, \
              frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at) \
         VALUES ($1, $2, 'EARN'::love_point_tx_type_enum, $3, $4, $5, \
                 $6, $6, 'ORDER_REWARD_REVIEW', $7, $8, NOW())",
    )
    .bind(user_id)
    .bind(group_id)
    .bind(reward)
    .bind(before)
    .bind(after)
    .bind(frozen)
    .bind(order_id)
    .bind(format!("admin_order_{order_id}_point"))
    .execute(&mut **tx)
    .await?;
    Ok(reward)
}

async fn grant_exp_reward(
    tx: &mut Transaction<'_, Postgres>,
    order_id: i64,
    group_id: i64,
    reward: i32,
) -> Result<i64, CustomError> {
    if reward <= 0 {
        return Ok(0);
    }
    let already_granted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM group_exp_transactions \
         WHERE biz_id = $1 AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW') \
           AND type = 'EARN'::group_exp_tx_type_enum)",
    )
    .bind(order_id)
    .fetch_one(&mut **tx)
    .await?;
    if already_granted {
        return Ok(0);
    }
    let (before, level_before): (i64, i32) =
        sqlx::query_as("SELECT exp, level FROM association_groups WHERE group_id = $1 FOR UPDATE")
            .bind(group_id)
            .fetch_one(&mut **tx)
            .await?;
    let after = before
        .checked_add(i64::from(reward))
        .ok_or_else(|| CustomError::internal("组经验溢出"))?;
    let level_after: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(level), 1) FROM group_level_configs WHERE required_exp <= $1",
    )
    .bind(after)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query("UPDATE association_groups SET exp = $2, level = $3, updated_at = NOW() WHERE group_id = $1")
        .bind(group_id)
        .bind(after)
        .bind(level_after)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO group_exp_transactions \
             (group_id, type, amount, exp_before, exp_after, level_before, level_after, \
              biz_type, biz_id, idempotency_key, created_at) \
         VALUES ($1, 'EARN'::group_exp_tx_type_enum, $2, $3, $4, $5, $6, \
                 'ORDER_REWARD_REVIEW', $7, $8, NOW())",
    )
    .bind(group_id)
    .bind(reward)
    .bind(before)
    .bind(after)
    .bind(level_before)
    .bind(level_after)
    .bind(order_id)
    .bind(format!("admin_order_{order_id}_exp"))
    .execute(&mut **tx)
    .await?;
    Ok(i64::from(reward))
}

#[utoipa::path(
    post,
    path = "/api/admin/orders/{order_id}/reward-review",
    tag = "后台管理",
    params(("order_id" = i64, Path)),
    request_body = AdminOrderReviewInput,
    responses((status = 200, body = AdminOrderReviewResponse), (status = 404)),
    security(("bearer_auth" = []))
)]
pub async fn review_order_rewards(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
    body: Json<AdminOrderReviewInput>,
) -> Result<impl Responder, CustomError> {
    require_admin_role(&admin, &[AdminRole::RiskReviewer])?;
    let order_id = path.into_inner();
    let input = body.into_inner();
    let mut tx = state.db_pool.begin().await?;
    let row = sqlx::query(
        "SELECT point_grant_status::text AS point_status, \
                exp_grant_status::text AS exp_status, group_id, assignee_id, \
                points_reward, group_exp_reward \
         FROM orders WHERE order_id = $1 FOR UPDATE",
    )
    .bind(order_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| CustomError::order_not_found("订单不存在"))?;
    let point_before: String = row.get("point_status");
    let exp_before: String = row.get("exp_status");
    if point_before != "PENDING_REVIEW" && exp_before != "PENDING_REVIEW" {
        tx.commit().await?;
        return Ok(ApiResponse::success(AdminOrderReviewResponse {
            order_id,
            point_grant_status: point_before,
            exp_grant_status: exp_before,
            point_amount_granted: 0,
            exp_amount_granted: 0,
            already_reviewed: true,
        }));
    }
    let rejects_pending_reward = (point_before == "PENDING_REVIEW" && !input.approve_point)
        || (exp_before == "PENDING_REVIEW" && !input.approve_exp);
    if rejects_pending_reward
        && input
            .remark
            .as_deref()
            .map(str::trim)
            .filter(|remark| !remark.is_empty())
            .is_none()
    {
        return Err(CustomError::BadRequest("拒绝奖励时必须填写原因".into()));
    }
    let group_id: i64 = row.get("group_id");
    let assignee_id: Option<i64> = row.try_get("assignee_id").ok().flatten();
    let points_reward: i32 = row.get("points_reward");
    let exp_reward: i32 = row.get("group_exp_reward");
    let mut point_amount_granted = 0;
    let mut exp_amount_granted = 0;
    let point_after = if point_before == "PENDING_REVIEW" {
        if input.approve_point {
            let receiver =
                assignee_id.ok_or_else(|| CustomError::BadRequest("订单缺少奖励接收人".into()))?;
            point_amount_granted =
                grant_point_reward(&mut tx, order_id, group_id, receiver, points_reward).await?;
            "GRANTED"
        } else {
            "REJECTED"
        }
    } else {
        point_before.as_str()
    };
    let exp_after = if exp_before == "PENDING_REVIEW" {
        if input.approve_exp {
            exp_amount_granted = grant_exp_reward(&mut tx, order_id, group_id, exp_reward).await?;
            "GRANTED"
        } else {
            "REJECTED"
        }
    } else {
        exp_before.as_str()
    };
    sqlx::query(
        "UPDATE orders SET point_grant_status = $2::point_grant_status_enum, \
                exp_grant_status = $3::exp_grant_status_enum, updated_at = NOW() \
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(point_after)
    .bind(exp_after)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO audit_logs \
             (operator_id, operator_type, action_type, target_type, target_id, detail, created_at) \
         VALUES ($1, 'ADMIN', 'ORDER_REWARD_REVIEW', 'ORDER', $2, $3, NOW())",
    )
    .bind(admin.user_id)
    .bind(order_id)
    .bind(serde_json::json!({
        "before": {
            "pointGrantStatus": point_before,
            "expGrantStatus": exp_before,
        },
        "after": {
            "pointGrantStatus": point_after,
            "expGrantStatus": exp_after,
        },
        "pointAmountGranted": point_amount_granted,
        "expAmountGranted": exp_amount_granted,
        "remark": input.remark,
    }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(ApiResponse::success(AdminOrderReviewResponse {
        order_id,
        point_grant_status: point_after.to_string(),
        exp_grant_status: exp_after.to_string(),
        point_amount_granted,
        exp_amount_granted,
        already_reviewed: false,
    }))
}
