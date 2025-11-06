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
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GroupActivityEventOut {
    pub event_type: String,
    pub actor_user_id: Option<i64>,
    pub ref_id: Option<i64>,
    pub ref_name: Option<String>,
    pub occurred_at: DateTime<Utc>,
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
    user_token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<GroupActivityQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 简单权限：确认组存在（若无则返回错误）
    let g_exists = sqlx::query("SELECT group_id FROM association_groups WHERE group_id=$1")
        .bind(*group_id)
        .fetch_optional(db)
        .await?;
    if g_exists.is_none() {
        return Err(CustomError::BadRequest("关联组不存在".into()));
    }
    // 聚合事件：订单与菜品
    let sql = format!(
        "SELECT order_id AS ref_id, user_id AS actor_user_id, 'ORDER_CREATED' AS event_type, created_at AS occurred_at, NULL::text AS ref_name FROM orders WHERE group_id=$1
        UNION ALL
        SELECT order_id, receiver_id, 'ORDER_ACCEPTED', last_status_change_at, NULL FROM orders WHERE group_id=$1 AND status='ACCEPTED' AND last_status_change_at IS NOT NULL
        UNION ALL
        SELECT order_id, receiver_id, 'ORDER_FINISHED', last_status_change_at, NULL FROM orders WHERE group_id=$1 AND status='FINISHED' AND last_status_change_at IS NOT NULL
        UNION ALL
        SELECT food_id, created_by, CASE WHEN submit_role='ORDERING_APPLY' THEN 'FOOD_APPLIED' ELSE 'FOOD_CREATED' END, created_at, food_name FROM foods WHERE group_id=$1
        ORDER BY occurred_at DESC;"
    );
    let rows = sqlx::query(&sql).bind(*group_id).fetch_all(db).await?;
    let list: Vec<GroupActivityEventOut> = rows
        .into_iter()
        .map(|r| GroupActivityEventOut {
            event_type: r.get::<String, _>("event_type"),
            actor_user_id: r.try_get("actor_user_id").ok(),
            ref_id: r.try_get("ref_id").ok(),
            ref_name: r.try_get("ref_name").ok(),
            occurred_at: r.get::<DateTime<Utc>, _>("occurred_at"),
        })
        .collect();
    Ok(HttpResponse::Ok().json(&list))
}
