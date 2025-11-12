use crate::{errors::CustomError, models::users::UserToken, AppState};
use chrono::{DateTime, Utc};
use ntex::web::{
    types::{Path, Query, State},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct GroupActivityQuery {
    /// 返回条数，默认50，最大200
    pub limit: Option<i64>,
    /// 仅返回该时间点之前的事件（用于下拉分页）
    pub before: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GroupActivityEventOut {
    pub event_type: String,
    pub actor_user_id: Option<i64>,
    pub ref_id: Option<i64>,
    pub ref_name: Option<String>,
    pub occurred_at: DateTime<Utc>,
    /// 积分流水相关：本次变动的积分值（正增负减）
    pub point_amount: Option<i32>,
    /// 积分类型（枚举值文本）
    pub point_tx_type: Option<String>,
    /// 变动后余额
    pub point_balance_after: Option<i32>,
}

#[utoipa::path(
    get, 
    path="/groups/{group_id}/activities", 
    tag="看板", 
    params(
        ("group_id"=i64, Path, description="组ID")
    ), 
    responses((
        status=200, 
        body=[GroupActivityEventOut]
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
    // 1. 校验组存在
    let g_exists = sqlx::query("SELECT 1 FROM association_groups WHERE group_id=$1")
        .bind(*group_id)
        .fetch_optional(db)
        .await?;
    if g_exists.is_none() {
        return Err(CustomError::BadRequest("关联组不存在".into()));
    }

    // 分页参数
    let before_ts = query.before.unwrap_or_else(Utc::now);
    let mut limit = query.limit.unwrap_or(50);
    if limit <= 0 { limit = 50; }
    if limit > 200 { limit = 200; }

    // 2. 聚合事件 (UNION ALL)
    // 说明：
    // ORDER_CREATED      -> 订单创建（聚合菜名）
    // ORDER_ACCEPTED     -> 状态历史里 to_status=ACCEPTED
    // ORDER_FINISHED     -> 状态历史里 to_status=FINISHED
    // FOOD_APPLIED       -> 菜品申请（submit_role=ORDERING_APPLY）
    // FOOD_CREATED       -> RECEIVING 创建的菜品
    // FOOD_APPROVED      -> 审核通过（food_audit_logs.action=2）
    // WISH_REDEEMED      -> 组内成员的心愿兑换（wish_claims，通过成员关系归属组）
    // （后续可追加：评分、抽奖等）

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
            WHERE o.group_id=$1 AND o.created_at < $2
            GROUP BY o.order_id, o.user_id, o.created_at

            UNION ALL
            -- 订单接单 / 完成（来自状态历史，保持事件留存）
            SELECT 
                osh.order_id AS ref_id,
                osh.changed_by AS actor_user_id,
                CASE WHEN osh.to_status='ACCEPTED' THEN 'ORDER_ACCEPTED' ELSE 'ORDER_FINISHED' END AS event_type,
                osh.changed_at AS occurred_at,
                NULL::text AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM order_status_history osh
            JOIN orders o ON osh.order_id=o.order_id
            WHERE o.group_id=$1
              AND osh.to_status IN ('ACCEPTED','FINISHED')
              AND osh.changed_at < $2

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
            WHERE f.group_id=$1 AND f.created_at < $2

            UNION ALL
            -- 菜品审核通过
            SELECT 
                fal.food_id AS ref_id,
                fal.acted_by AS actor_user_id,
                'FOOD_APPROVED' AS event_type,
                fal.created_at AS occurred_at,
                f.food_name AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM food_audit_logs fal
            JOIN foods f ON f.food_id=fal.food_id
            WHERE f.group_id=$1 AND fal.action=2 AND fal.created_at < $2

            UNION ALL
            -- 心愿兑换（组内成员）
            SELECT 
                wc.id AS ref_id,
                wc.user_id AS actor_user_id,
                'WISH_REDEEMED' AS event_type,
                wc.created_at AS occurred_at,
                w.wish_name AS ref_name,
                NULL::int AS point_amount,
                NULL::text AS point_tx_type,
                NULL::int AS point_balance_after
            FROM wish_claims wc
            JOIN wishes w ON w.wish_id=wc.wish_id
            JOIN association_group_members agm ON agm.user_id=wc.user_id AND agm.group_id=$1
            WHERE wc.created_at < $2

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
                    ELSE 'POINT_OTHER'
                END AS event_type,
                pt.created_at AS occurred_at,
                NULL::text AS ref_name,
                pt.amount AS point_amount,
                pt.type::text AS point_tx_type,
                pt.balance_after AS point_balance_after
            FROM point_transactions pt
            JOIN association_group_members agm ON agm.user_id=pt.user_id AND agm.group_id=$1
            WHERE pt.created_at < $2
        ) all_events
        ORDER BY occurred_at DESC
        LIMIT $3
    "#;

    let rows = sqlx::query(sql)
        .bind(*group_id)
        .bind(before_ts)
        .bind(limit)
        .fetch_all(db)
        .await?;

    let list: Vec<GroupActivityEventOut> = rows.into_iter().map(|r| GroupActivityEventOut {
        event_type: r.get::<String,_>("event_type"),
        actor_user_id: r.try_get("actor_user_id").ok(),
        ref_id: r.try_get("ref_id").ok(),
        ref_name: r.try_get("ref_name").ok(),
        occurred_at: r.get::<DateTime<Utc>,_>("occurred_at"),
        point_amount: r.try_get("point_amount").ok(),
        point_tx_type: r.try_get("point_tx_type").ok(),
        point_balance_after: r.try_get("point_balance_after").ok(),
    }).collect();

    Ok(HttpResponse::Ok().json(&list))
}
