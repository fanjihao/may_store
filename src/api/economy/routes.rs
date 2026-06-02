// API - 经济查询路由
// FSD.latest.md compliant - 积分/钻石/经验流水查询

use ntex::web::{self, HttpResponse, ServiceConfig, types::{Path, Query, State}};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置经济查询路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups/{group_id}")
            .route("/points", web::get().to(get_points))
            .route("/transactions", web::get().to(get_transactions))
            .route("/exp", web::get().to(get_group_exp)),
    );
}

/// 获取用户组内积分
/// GET /api/groups/{group_id}/points?user_id=xxx
///
/// 返回:
/// - available_love_point: 可用爱心积分
/// - frozen_love_point: 冻结爱心积分
async fn get_points(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<PointsQuery>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = query.user_id.unwrap_or(token.user_id);

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 查询用户组内积分
    let points: Option<(i64, i64)> = sqlx::query_as(
        "SELECT available_love_point, frozen_love_point FROM user_group_points WHERE user_id=$1 AND group_id=$2"
    )
    .bind(user_id)
    .bind(gid)
    .fetch_optional(db)
    .await?;

    let (available, frozen) = points.unwrap_or((0, 0));

    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "userId": user_id,
        "groupId": gid,
        "availableLovePoint": available,
        "frozenLovePoint": frozen,
        "status": "ok"
    })))
}

/// 获取积分流水
/// GET /api/groups/{group_id}/transactions?user_id=xxx&type=EARN&cursor=xxx&limit=20
async fn get_transactions(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<TransactionsQuery>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = query.user_id.unwrap_or(token.user_id);
    let tx_type = &query.tx_type;
    let limit = query.limit.unwrap_or(20).min(100);

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 构建查询
    let transactions = if let Some(tx_type) = tx_type {
        sqlx::query_as::<_, TransactionRecord>(
            r#"SELECT id, user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, created_at
               FROM love_point_transactions
               WHERE user_id=$1 AND group_id=$2 AND type=$3
               ORDER BY created_at DESC
               LIMIT $4"#
        )
        .bind(user_id)
        .bind(gid)
        .bind(tx_type)
        .bind(limit)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as::<_, TransactionRecord>(
            r#"SELECT id, user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, created_at
               FROM love_point_transactions
               WHERE user_id=$1 AND group_id=$2
               ORDER BY created_at DESC
               LIMIT $3"#
        )
        .bind(user_id)
        .bind(gid)
        .bind(limit)
        .fetch_all(db)
        .await?
    };

    let result: Vec<serde_json::Value> = transactions.into_iter().map(|t| {
        serde_json::json!({
            "id": t.id,
            "userId": t.user_id,
            "groupId": t.group_id,
            "type": t.type_,
            "amount": t.amount,
            "availableBefore": t.available_before,
            "availableAfter": t.available_after,
            "frozenBefore": t.frozen_before,
            "frozenAfter": t.frozen_after,
            "bizType": t.biz_type,
            "bizId": t.biz_id,
            "createdAt": t.created_at
        })
    }).collect();

    Ok(HttpResponse::Ok().json(&result))
}

/// 获取组经验信息
/// GET /api/groups/{group_id}/exp
///
/// 返回:
/// - level: 组等级
/// - exp: 当前经验
/// - exp_to_next_level: 到下一级还需经验
async fn get_group_exp(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 查询组经验信息
    let exp_info: Option<(i32, i64)> = sqlx::query_as(
        "SELECT level, exp FROM association_groups WHERE group_id=$1"
    )
    .bind(gid)
    .fetch_optional(db)
    .await?;

    let (level, exp) = exp_info.unwrap_or((1, 0));

    // 计算到下一级还需经验（简化：每级需要 level * 100 经验）
    let exp_for_next_level = level * 100;
    let exp_to_next = (level * 100 - exp as i32).max(0) as i64;

    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "groupId": gid,
        "level": level,
        "exp": exp,
        "expForNextLevel": exp_for_next_level,
        "expToNextLevel": exp_to_next,
        "status": "ok"
    })))
}

// ============== FSD v2 结构体 ==============

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PointsQuery {
    pub user_id: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsQuery {
    pub user_id: Option<i64>,
    pub tx_type: Option<String>,  // EARN, FREEZE, UNFREEZE, DEDUCT, ADJUST
    pub cursor: Option<String>,
    pub limit: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct TransactionRecord {
    id: i64,
    user_id: i64,
    group_id: i64,
    #[sqlx(rename = "type")]
    type_: String,
    amount: i64,
    available_before: i64,
    available_after: i64,
    frozen_before: i64,
    frozen_after: i64,
    biz_type: String,
    biz_id: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}