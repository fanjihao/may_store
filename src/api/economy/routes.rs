// API - 经济查询路由
// FSD.latest.md compliant - 积分/钻石/经验流水查询

use ntex::web::{
    self,
    types::{Path, Query, State},
    HttpResponse, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::utils::response::ApiResponse;

/// 配置经济查询路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups/{group_id}")
            // 爱心积分
            .route("/points/balance", web::get().to(get_points_balance))
            .route("/points/transactions", web::get().to(get_points_transactions))
            // 钻石
            .route("/diamonds/balance", web::get().to(get_diamonds_balance))
            .route("/diamonds/transactions", web::get().to(get_diamonds_transactions))
            // 组经验
            .route("/exp", web::get().to(get_group_exp))
            .route("/exp/transactions", web::get().to(get_exp_transactions)),
    );
}

// ========== 响应结构 ==========

/// 爱心积分余额响应 (FSD v2 8.1)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PointsBalanceResponse {
    pub group_id: i64,
    pub user_id: i64,
    pub available_love_point: i64,
    pub frozen_love_point: i64,
    pub daily_love_point_limit: Option<i32>,
    pub today_love_point_earned: Option<i32>,
    pub today_love_point_remaining: Option<i32>,
}

/// 爱心积分流水项 (FSD v2 8.2)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PointsTransactionItem {
    pub id: i64,
    #[serde(rename = "type")]
    pub type_: String,
    pub amount: i64,
    pub available_before: i64,
    pub available_after: i64,
    pub frozen_before: i64,
    pub frozen_after: i64,
    pub biz_type: String,
    pub biz_id: Option<i64>,
    pub idempotency_key: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 积分流水列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct PointsTransactionsResponse {
    pub transactions: Vec<PointsTransactionItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// 钻石余额响应 (FSD v2 8.3)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiamondsBalanceResponse {
    pub group_id: i64,
    pub diamond: i64,
    pub diamond_capacity: Option<i64>,
    pub today_diamond_earned: Option<i32>,
}

/// 钻石流水项 (FSD v2 8.4)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiamondsTransactionItem {
    pub id: i64,
    #[serde(rename = "type")]
    pub type_: String,
    pub amount: i64,
    pub balance_before: i64,
    pub balance_after: i64,
    pub biz_type: String,
    pub biz_id: Option<i64>,
    pub idempotency_key: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 钻石流水列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DiamondsTransactionsResponse {
    pub transactions: Vec<DiamondsTransactionItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// 组经验响应 (FSD v2 8.5)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupExpResponse {
    pub group_id: i64,
    pub level: i32,
    pub exp: i64,
    pub next_level_exp: Option<i64>,
    pub progress: Option<f64>,
    pub daily_group_exp_limit: Option<i32>,
    pub today_group_exp_earned: Option<i32>,
    pub today_group_exp_remaining: Option<i32>,
}

/// 组经验流水项 (FSD v2 8.6)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpTransactionItem {
    pub id: i64,
    #[serde(rename = "type")]
    pub type_: String,
    pub amount: i64,
    pub exp_before: i64,
    pub exp_after: i64,
    pub level_before: i32,
    pub level_after: i32,
    pub biz_type: String,
    pub biz_id: Option<i64>,
    pub idempotency_key: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 经验流水列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct ExpTransactionsResponse {
    pub transactions: Vec<ExpTransactionItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

// ========== 处理器 ==========

/// 获取爱心积分余额
/// GET /api/groups/{group_id}/points/balance
/// FSD v2 8.1
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/points/balance",
    tag = "经济查询",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = PointsBalanceResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn get_points_balance(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
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

    // 获取每日限额信息（简化：默认100）
    let daily_limit = 100;

    // 获取今日获取积分（简化：从流水汇总）
    let today_earned: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(SUM(amount), 0) FROM love_point_transactions
           WHERE user_id=$1 AND group_id=$2 AND type='EARN'
           AND created_at >= CURRENT_DATE"#
    )
    .bind(user_id)
    .bind(gid)
    .fetch_one(db)
    .await?;

    let today_remaining = (daily_limit as i64 - today_earned).max(0);

    Ok(ApiResponse::success(PointsBalanceResponse {
        group_id: gid,
        user_id,
        available_love_point: available,
        frozen_love_point: frozen,
        daily_love_point_limit: Some(daily_limit),
        today_love_point_earned: Some(today_earned as i32),
        today_love_point_remaining: Some(today_remaining as i32),
    }))
}

/// 获取爱心积分流水
/// GET /api/groups/{group_id}/points/transactions
/// FSD v2 8.2
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/points/transactions",
    tag = "经济查询",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        TransactionsQuery
    ),
    responses(
        (status = 200, description = "获取成功", body = PointsTransactionsResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn get_points_transactions(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<TransactionsQuery>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;
    let tx_type = &query.tx_type;
    let limit = query.limit.unwrap_or(20).min(100);

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 参数化查询 + 枚举白名单,避免 SQL 注入
    let tx_type_filter: Option<&str> = match tx_type.as_deref() {
        Some(t) if matches!(t, "EARN" | "FREEZE" | "UNFREEZE" | "DEDUCT" | "ADJUST") => Some(t),
        Some(_) => return Err(CustomError::BadRequest("tx_type 非法".into())),
        None => None,
    };

    let rows = match tx_type_filter {
        Some(t) => sqlx::query(
            r#"SELECT id, type, amount, available_before, available_after, frozen_before, frozen_after,
                      biz_type, biz_id, idempotency_key, created_at
               FROM love_point_transactions
               WHERE user_id=$1 AND group_id=$2 AND type=$3
               ORDER BY created_at DESC
               LIMIT $4"#,
        )
        .bind(user_id)
        .bind(gid)
        .bind(t)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
        None => sqlx::query(
            r#"SELECT id, type, amount, available_before, available_after, frozen_before, frozen_after,
                      biz_type, biz_id, idempotency_key, created_at
               FROM love_point_transactions
               WHERE user_id=$1 AND group_id=$2
               ORDER BY created_at DESC
               LIMIT $3"#,
        )
        .bind(user_id)
        .bind(gid)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
    };

    let has_more = rows.len() > limit as usize;
    let transactions: Vec<PointsTransactionItem> = rows
        .iter()
        .take(limit as usize)
        .map(|r| {
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            PointsTransactionItem {
                id: r.get("id"),
                type_: r.get("type"),
                amount: r.get("amount"),
                available_before: r.get("available_before"),
                available_after: r.get("available_after"),
                frozen_before: r.get("frozen_before"),
                frozen_after: r.get("frozen_after"),
                biz_type: r.get("biz_type"),
                biz_id: r.get("biz_id"),
                idempotency_key: r.get("idempotency_key"),
                created_at,
            }
        })
        .collect();

    let next_cursor = if has_more {
        transactions.last().map(|t| t.id.to_string())
    } else {
        None
    };

    Ok(ApiResponse::success(PointsTransactionsResponse {
        transactions,
        next_cursor,
        has_more,
    }))
}

/// 获取钻石余额
/// GET /api/groups/{group_id}/diamonds/balance
/// FSD v2 8.3
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/diamonds/balance",
    tag = "经济查询",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = DiamondsBalanceResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn get_diamonds_balance(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 查询组钻石
    let group_info: Option<(i64, i32)> = sqlx::query_as(
        "SELECT diamond, footprint_capacity FROM association_groups WHERE group_id=$1"
    )
    .bind(gid)
    .fetch_optional(db)
    .await?;

    let (diamond, footprint_capacity) = group_info.unwrap_or((0, 50));

    // 获取今日获取钻石
    let today_earned: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(SUM(amount), 0) FROM diamond_transactions
           WHERE group_id=$1 AND type='EARN'
           AND created_at >= CURRENT_DATE"#
    )
    .bind(gid)
    .fetch_one(db)
    .await?;

    Ok(ApiResponse::success(DiamondsBalanceResponse {
        group_id: gid,
        diamond: diamond as i64,
        diamond_capacity: Some(footprint_capacity as i64),
        today_diamond_earned: Some(today_earned as i32),
    }))
}

/// 获取钻石流水
/// GET /api/groups/{group_id}/diamonds/transactions
/// FSD v2 8.4
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/diamonds/transactions",
    tag = "经济查询",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        DiamondTransactionsQuery
    ),
    responses(
        (status = 200, description = "获取成功", body = DiamondsTransactionsResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn get_diamonds_transactions(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<DiamondTransactionsQuery>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;
    let tx_type = &query.tx_type;
    let limit = query.limit.unwrap_or(20).min(100);

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 参数化查询 + 枚举白名单
    let tx_type_filter: Option<&str> = match query.tx_type.as_deref() {
        Some(t) if matches!(t, "EARN" | "CONSUME" | "ADJUST") => Some(t),
        Some(_) => return Err(CustomError::BadRequest("tx_type 非法".into())),
        None => None,
    };

    let rows = match tx_type_filter {
        Some(t) => sqlx::query(
            r#"SELECT id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, created_at
               FROM diamond_transactions
               WHERE group_id=$1 AND type=$2
               ORDER BY created_at DESC
               LIMIT $3"#,
        )
        .bind(gid)
        .bind(t)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
        None => sqlx::query(
            r#"SELECT id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, created_at
               FROM diamond_transactions
               WHERE group_id=$1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(gid)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
    };

    let has_more = rows.len() > limit as usize;
    let transactions: Vec<DiamondsTransactionItem> = rows
        .iter()
        .take(limit as usize)
        .map(|r| {
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            DiamondsTransactionItem {
                id: r.get("id"),
                type_: r.get("type"),
                amount: r.get("amount"),
                balance_before: r.get("balance_before"),
                balance_after: r.get("balance_after"),
                biz_type: r.get("biz_type"),
                biz_id: r.get("biz_id"),
                idempotency_key: r.get("idempotency_key"),
                created_at,
            }
        })
        .collect();

    let next_cursor = if has_more {
        transactions.last().map(|t| t.id.to_string())
    } else {
        None
    };

    Ok(ApiResponse::success(DiamondsTransactionsResponse {
        transactions,
        next_cursor,
        has_more,
    }))
}

/// 获取组经验信息
/// GET /api/groups/{group_id}/exp
/// FSD v2 8.5
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/exp",
    tag = "经济查询",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = GroupExpResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn get_group_exp(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 查询组经验信息
    let exp_info: Option<(i32, i64)> =
        sqlx::query_as("SELECT level, exp FROM association_groups WHERE group_id=$1")
            .bind(gid)
            .fetch_optional(db)
            .await?;

    let (level, exp) = exp_info.unwrap_or((1, 0));

    // 计算下一级经验（简化：每级需要 level * 100 经验）
    let next_level_exp = (level as i64 + 1) * 100;
    let progress = exp as f64 / next_level_exp as f64;

    // 获取今日获取经验
    let today_exp: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(SUM(amount), 0) FROM group_exp_transactions
           WHERE group_id=$1 AND type='EARN'
           AND created_at >= CURRENT_DATE"#
    )
    .bind(gid)
    .fetch_one(db)
    .await?;

    // 每日经验限额（简化：默认200）
    let daily_limit = 200;
    let today_remaining = (daily_limit as i64 - today_exp).max(0);

    Ok(ApiResponse::success(GroupExpResponse {
        group_id: gid,
        level,
        exp,
        next_level_exp: Some(next_level_exp),
        progress: Some(progress),
        daily_group_exp_limit: Some(daily_limit),
        today_group_exp_earned: Some(today_exp as i32),
        today_group_exp_remaining: Some(today_remaining as i32),
    }))
}

/// 获取组经验流水
/// GET /api/groups/{group_id}/exp/transactions
/// FSD v2 8.6
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/exp/transactions",
    tag = "经济查询",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        ExpTransactionsQuery
    ),
    responses(
        (status = 200, description = "获取成功", body = ExpTransactionsResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn get_exp_transactions(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<ExpTransactionsQuery>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;
    let tx_type = &query.tx_type;
    let limit = query.limit.unwrap_or(20).min(100);

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 参数化查询 + 枚举白名单
    let tx_type_filter: Option<&str> = match query.tx_type.as_deref() {
        Some(t) if matches!(t, "EARN" | "ADJUST" | "REVOKE") => Some(t),
        Some(_) => return Err(CustomError::BadRequest("tx_type 非法".into())),
        None => None,
    };

    let rows = match tx_type_filter {
        Some(t) => sqlx::query(
            r#"SELECT id, type, amount, exp_before, exp_after, level_before, level_after, biz_type, biz_id, idempotency_key, created_at
               FROM group_exp_transactions
               WHERE group_id=$1 AND type=$2
               ORDER BY created_at DESC
               LIMIT $3"#,
        )
        .bind(gid)
        .bind(t)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
        None => sqlx::query(
            r#"SELECT id, type, amount, exp_before, exp_after, level_before, level_after, biz_type, biz_id, idempotency_key, created_at
               FROM group_exp_transactions
               WHERE group_id=$1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(gid)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
    };

    let has_more = rows.len() > limit as usize;
    let transactions: Vec<ExpTransactionItem> = rows
        .iter()
        .take(limit as usize)
        .map(|r| {
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            ExpTransactionItem {
                id: r.get("id"),
                type_: r.get("type"),
                amount: r.get("amount"),
                exp_before: r.get("exp_before"),
                exp_after: r.get("exp_after"),
                level_before: r.get("level_before"),
                level_after: r.get("level_after"),
                biz_type: r.get("biz_type"),
                biz_id: r.get("biz_id"),
                idempotency_key: r.get("idempotency_key"),
                created_at,
            }
        })
        .collect();

    let next_cursor = if has_more {
        transactions.last().map(|t| t.id.to_string())
    } else {
        None
    };

    Ok(ApiResponse::success(ExpTransactionsResponse {
        transactions,
        next_cursor,
        has_more,
    }))
}

// ========== 查询参数 ==========

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsQuery {
    #[param(default)]
    pub tx_type: Option<String>, // EARN, FREEZE, UNFREEZE, DEDUCT, ADJUST
    pub cursor: Option<String>,
    #[param(default = 20)]
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DiamondTransactionsQuery {
    #[param(default)]
    pub tx_type: Option<String>, // EARN, CONSUME, ADJUST
    pub cursor: Option<String>,
    #[param(default = 20)]
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ExpTransactionsQuery {
    #[param(default)]
    pub tx_type: Option<String>, // EARN, ADJUST, REVOKE
    pub cursor: Option<String>,
    #[param(default = 20)]
    pub limit: Option<i32>,
}
