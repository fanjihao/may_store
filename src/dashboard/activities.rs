use crate::{
    errors::CustomError,
    models::{
        dashboard::{GroupActivityEventOut, GroupActivityQuery},
        pagination::{decode_cursor, encode_cursor, CursorPage},
        users::UserToken,
    },
    AppState,
};
use chrono::{DateTime, Utc};
use ntex::web::{
    types::{Path, Query, State},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize)]
pub struct ActivityCursor {
    pub occurred_at: DateTime<Utc>,
    pub ref_id: i64,
}

#[utoipa::path(
    get,
    path="/groups/{group_id}/activities",
    tag="看板",
    params(
        ("group_id"=i64, Path, description="组ID"),
        GroupActivityQuery
    ),
    responses((
        status=200,
        body=CursorPage<GroupActivityEventOut>
    )),
    security(("cookie_auth"=[]))
)]
pub async fn get_group_activities(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<GroupActivityQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    // 1. 校验组存在
    let g_exists = sqlx::query("SELECT 1 FROM association_groups WHERE group_id=$1")
        .bind(*group_id)
        .fetch_optional(db)
        .await?;
    if g_exists.is_none() {
        return Err(CustomError::BadRequest("关联组不存在".into()));
    }

    let cursor = query.cursor.as_deref().and_then(decode_cursor::<ActivityCursor>);

    // 2. 聚合事件 (UNION ALL)
    let sql = r#"
        SELECT * FROM (
            -- 订单创建
            SELECT
                o.order_id AS ref_id,
                o.user_id AS actor_user_id,
                'ORDER_CREATED' AS event_type,
                o.created_at AS occurred_at,
                STRING_AGG(f.food_name,'+') AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM orders o
            LEFT JOIN order_items oi ON o.order_id=oi.order_id
            LEFT JOIN foods f ON oi.food_id=f.food_id
            WHERE o.group_id=$1
            GROUP BY o.order_id, o.user_id, o.created_at

            UNION ALL
            -- 订单接单 / 完成 / 取消
            SELECT
                osh.order_id AS ref_id,
                osh.changed_by AS actor_user_id,
                CASE
                    WHEN osh.to_status='ACCEPTED' THEN 'ORDER_ACCEPTED'
                    WHEN osh.to_status='FINISHED' THEN 'ORDER_FINISHED'
                    WHEN osh.to_status='CANCELLED' THEN 'ORDER_CANCELED'
                    ELSE 'ORDER_OTHER'
                END AS event_type,
                osh.changed_at AS occurred_at,
                NULL::text AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM order_status_history osh
            JOIN orders o ON osh.order_id=o.order_id
            WHERE o.group_id=$1
              AND osh.to_status IN ('ACCEPTED','FINISHED','CANCELLED')

            UNION ALL
            -- 菜品申请 & 创建
            SELECT
                f.food_id AS ref_id,
                f.created_by AS actor_user_id,
                CASE WHEN f.submit_role='ORDERING_APPLY' THEN 'FOOD_APPLIED' ELSE 'FOOD_CREATED' END AS event_type,
                f.created_at AS occurred_at,
                f.food_name AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM foods f
            WHERE f.group_id=$1

            UNION ALL
            -- 菜品审核通过 / 拒绝
            SELECT
                fal.food_id AS ref_id,
                fal.acted_by AS actor_user_id,
                CASE WHEN fal.action=2 THEN 'FOOD_APPROVED' ELSE 'FOOD_REJECTED' END AS event_type,
                fal.created_at AS occurred_at,
                f.food_name AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM food_audit_logs fal
            JOIN foods f ON f.food_id=fal.food_id
            WHERE f.group_id=$1 AND fal.action IN (2, 3)

            UNION ALL
            -- 心愿创建
            SELECT
                w.wish_id AS ref_id,
                NULL::bigint AS actor_user_id,
                'WISH_CREATED' AS event_type,
                w.created_at AS occurred_at,
                w.wish_name AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM wishes w
            WHERE w.created_by=$1

            UNION ALL
            -- 心愿兑换（组内成员）
            SELECT
                wc.id AS ref_id,
                wc.user_id AS actor_user_id,
                'WISH_CLAIMED' AS event_type,
                wc.created_at AS occurred_at,
                w.wish_name AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM wish_claims wc
            JOIN wishes w ON w.wish_id=wc.wish_id
            JOIN association_group_members agm ON agm.user_id=wc.user_id AND agm.group_id=$1

            UNION ALL
            -- 积分流水（组内成员）
            SELECT
                pt.id AS ref_id,
                pt.user_id AS actor_user_id,
                CASE
                    WHEN pt.type='ORDER_REWARD' THEN 'POINT_GAIN_ORDER'
                    WHEN pt.type='FINISH_REWARD' THEN 'POINT_GAIN_FINISH'
                    WHEN pt.type='WISH_COST' THEN 'POINT_COST_WISH'
                    WHEN pt.type='ORDER_RATING' THEN 'POINT_DELTA_RATING'
                    WHEN pt.type='ADMIN_ADJUST' THEN 'POINT_ADJUST_ADMIN'
                    WHEN pt.type='LOTTERY_REWARD' THEN 'POINT_GAIN_LOTTERY'
                    WHEN pt.type='SIGN_IN_REWARD' THEN 'POINT_GAIN_SIGN_IN'
                    ELSE 'POINT_OTHER'
                END AS event_type,
                pt.created_at AS occurred_at,
                NULL::text AS ref_name,
                pt.amount AS point_amount,
                pt.type::text AS point_tx_type,
                pt.balance_after AS point_balance_after
            FROM point_transactions pt
            JOIN association_group_members agm ON agm.user_id=pt.user_id AND agm.group_id=$1
            WHERE pt.type != 'SIGN_IN_REWARD'

            UNION ALL
            -- 签到记录（组内成员）
            SELECT
                sr.sign_id AS ref_id,
                sr.user_id AS actor_user_id,
                'SIGN_IN' AS event_type,
                sr.created_at AS occurred_at,
                NULL::text AS ref_name,
                sr.points_earned AS point_amount,
                'SIGN_IN_REWARD'::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM sign_records sr
            JOIN association_group_members agm ON agm.user_id=sr.user_id AND agm.group_id=$1
        ) all_events
        WHERE ($2::timestamptz IS NULL OR (occurred_at, ref_id) < ($2, $3))
        ORDER BY occurred_at DESC, ref_id DESC
        LIMIT $4
    "#;

    let rows = sqlx::query(sql)
        .bind(*group_id)
        .bind(cursor.as_ref().map(|c| c.occurred_at))
        .bind(cursor.as_ref().map(|c| c.ref_id))
        .bind(limit + 1)
        .fetch_all(db)
        .await?;

    let has_more = rows.len() > limit as usize;
    let mut rows = rows;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&ActivityCursor {
                occurred_at: r.get("occurred_at"),
                ref_id: r.get("ref_id"),
            })
        })
    } else {
        None
    };

    let items: Vec<GroupActivityEventOut> = rows.into_iter().map(|r| GroupActivityEventOut {
        event_type: r.get::<String,_>("event_type"),
        actor_user_id: r.try_get("actor_user_id").ok(),
        ref_id: r.try_get("ref_id").ok(),
        ref_name: r.try_get("ref_name").ok(),
        occurred_at: r.get::<DateTime<Utc>,_>("occurred_at"),
        point_amount: r.try_get("point_amount").ok(),
        point_tx_type: r.try_get("point_tx_type").ok(),
        point_balance_after: r.try_get("point_balance_after").ok(),
    }).collect();

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
    }))
}
