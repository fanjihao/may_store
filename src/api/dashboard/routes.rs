// API - 数据看板路由
// FSD.latest.md compliant - 用户组内看板、管理员运营看板、趋势数据

use chrono::{Datelike, NaiveDate};
use ntex::web::{
    self,
    types::{Path, Query, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::admin_auth::AdminToken;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::middlewares::target_group::require_active_target_group_member;
use crate::utils::response::ApiResponse;

/// 配置数据看板路由
pub fn configure(cfg: &mut ServiceConfig) {
    // 不用 web::scope —— 避免圈住路径
    // 用户组内看板
    cfg.service(
        web::resource("/api/groups/{group_id}/dashboard").route(web::get().to(get_group_dashboard)),
    );
    // 组活动流 (过往足迹) — bindDetail 页面用
    cfg.service(
        web::resource("/api/groups/{group_id}/activities")
            .route(web::get().to(get_group_activities)),
    );
    // 管理员看板(运营 + 趋势)
    cfg.service(web::resource("/api/admin/dashboard").route(web::get().to(get_admin_dashboard)));
    cfg.service(
        web::resource("/api/admin/dashboard/trends").route(web::get().to(get_dashboard_trends)),
    );
}

// ========== 响应结构 ==========

/// 用户组内看板响应 (FSD v2 16.1)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupDashboardResponse {
    pub group: GroupInfo,
    pub today: TodayStats,
    pub this_month: MonthStats,
    pub quick_stats: QuickStats,
}

/// 组信息
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub group_id: i64,
    pub name: String,
    pub level: i32,
    pub exp: i64,
    pub next_level_exp: i64,
    pub diamond: i32,
    pub daily_love_point_limit: i32,
    pub daily_group_exp_limit: i32,
    pub created_at: String, // RFC3339
}

/// 今日统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayStats {
    pub date: String,
    pub orders_completed: i32,
    pub love_points_earned: i32,
    pub group_exp_earned: i32,
    pub sign_in: serde_json::Value, // 组内双方签到状态
}

/// 本月统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthStats {
    pub orders_completed: i32,
    pub wishes_finished: i32,
    pub love_points_spent: i32,
    pub love_points_earned: i32,
    pub feeds_completed: i32,
}

/// 快速统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuickStats {
    pub total_orders: i32,
    pub total_wishes: i32,
    pub finished_wishes: i32,
    pub fulfillment_rate: f64,
    pub continuous_sign_in_days_user1: i32,
    pub continuous_sign_in_days_user2: i32,
    pub total_feeds: i32,   // 累计已完成订单数
    pub days_together: i32, // 相遇天数
    pub total_diamonds_earned: i64,
    pub total_diamonds_spent: i64,
    pub total_love_points_balance: i64, // 当前用户积分
}

/// 管理员运营看板响应 (FSD v2 16.2)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminDashboardResponse {
    pub overview: OverviewStats,
    pub order_stats: OrderStats,
    pub wish_stats: WishStats,
    pub economy_stats: EconomyStats,
    pub risk_stats: RiskStats,
    pub sign_in_stats: SignInStats,
}

/// 概览统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverviewStats {
    pub total_users: i64,
    pub total_groups: i64,
    pub dau: i64,
    pub new_users_today: i64,
    pub new_groups_today: i64,
    pub active_guest_orders_today: i64,
}

/// 订单统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderStats {
    pub total_orders_today: i64,
    pub completed_orders_today: i64,
    pub completion_rate: f64,
    pub avg_completion_hours: f64,
    pub guest_orders_today: i64,
    pub normal_orders_today: i64,
}

/// 心愿统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishStats {
    pub total_wishes: i64,
    pub active_wishes: i64,
    pub finished_wishes: i64,
    pub expired_wishes: i64,
    pub total_frozen_points: i64,
}

/// 经济统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EconomyStats {
    pub total_love_points_issued_today: i64,
    pub total_diamonds_spent_today: i64,
    pub total_group_exp_earned_today: i64,
    pub avg_love_point_per_order: f64,
}

/// 风控统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RiskStats {
    pub pending_review_orders: i64,
    pub suspected_fraud_orders: i64,
    pub banned_users_today: i64,
}

/// 签到统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInStats {
    pub total_sign_ins_today: i64,
    pub full_team_sign_ins_today: i64,
    pub avg_consecutive_days: f64,
}

/// 趋势数据响应 (FSD v2 16.3)
#[derive(Debug, Serialize, ToSchema)]
pub struct TrendsResponse {
    pub metric: String,
    pub granularity: String,
    pub data_points: Vec<DataPoint>,
}

/// 数据点
#[derive(Debug, Serialize, ToSchema)]
pub struct DataPoint {
    pub date: String,
    pub value: i64,
}

// ========== 查询参数 ==========

/// 趋势数据查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TrendsQuery {
    pub metric: String, // DAU / ORDERS / WISHES / LOVE_POINTS / SIGN_INS
    #[serde(alias = "start_date")]
    pub start_date: String,
    #[serde(alias = "end_date")]
    pub end_date: String,
    pub granularity: Option<String>, // DAY / HOUR，默认 DAY
}

/// 管理员看板查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct AdminDashboardQuery {
    #[serde(alias = "start_date")]
    pub start_date: Option<String>,
    #[serde(alias = "end_date")]
    pub end_date: Option<String>,
}

const MAX_DASHBOARD_RANGE_DAYS: i64 = 366;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrendMetric {
    Dau,
    Orders,
    Wishes,
    LovePoints,
    SignIns,
}

impl TrendMetric {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "DAU" => Some(Self::Dau),
            "ORDERS" => Some(Self::Orders),
            "WISHES" => Some(Self::Wishes),
            "LOVE_POINTS" => Some(Self::LovePoints),
            "SIGN_INS" => Some(Self::SignIns),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Dau => "DAU",
            Self::Orders => "ORDERS",
            Self::Wishes => "WISHES",
            Self::LovePoints => "LOVE_POINTS",
            Self::SignIns => "SIGN_INS",
        }
    }
}

fn parse_dashboard_date(value: &str) -> Option<NaiveDate> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return None;
    }

    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn validate_dashboard_date_range(
    start_date: &str,
    end_date: &str,
) -> Result<(NaiveDate, NaiveDate), &'static str> {
    let start = parse_dashboard_date(start_date)
        .ok_or("startDate 和 endDate 必须是 YYYY-MM-DD 格式的有效日期")?;
    let end = parse_dashboard_date(end_date)
        .ok_or("startDate 和 endDate 必须是 YYYY-MM-DD 格式的有效日期")?;

    if start > end {
        return Err("startDate 不能晚于 endDate");
    }
    if (end - start).num_days() + 1 > MAX_DASHBOARD_RANGE_DAYS {
        return Err("日期范围最多为 366 个自然日");
    }

    Ok((start, end))
}

fn resolve_admin_date_range(
    query: &AdminDashboardQuery,
    current_date: NaiveDate,
) -> Result<(NaiveDate, NaiveDate), &'static str> {
    match (query.start_date.as_deref(), query.end_date.as_deref()) {
        (None, None) => Ok((current_date, current_date)),
        (Some(start), None) => validate_dashboard_date_range(start, start),
        (None, Some(end)) => validate_dashboard_date_range(end, end),
        (Some(start), Some(end)) => validate_dashboard_date_range(start, end),
    }
}

fn dashboard_i32(value: i64, metric: &str) -> Result<i32, CustomError> {
    i32::try_from(value).map_err(|_| CustomError::internal(format!("{} 指标超出 i32 范围", metric)))
}

// ========== 处理器 ==========

/// 获取用户组内看板
/// GET /api/groups/{group_id}/dashboard
/// FSD v2 16.1
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/dashboard",
    tag = "数据看板",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = GroupDashboardResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_group_dashboard(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    group_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;
    let user_id = token.user_id;

    require_active_target_group_member(db, user_id, gid).await?;

    // 组等级/钻石是 INT，经验是 BIGINT；每日上限来自 group_configs。
    let group_info: Option<(
        String,
        i32,
        i64,
        i64,
        i32,
        i32,
        i32,
        chrono::DateTime<chrono::Utc>,
        Option<i64>,
        Option<i64>,
        NaiveDate,
    )> = sqlx::query_as(
        r#"
        SELECT
            COALESCE(g.group_name, '未命名组'),
            g.level,
            g.exp,
            COALESCE(
                (
                    SELECT glc.required_exp
                    FROM group_level_configs glc
                    WHERE glc.level > g.level
                    ORDER BY glc.level
                    LIMIT 1
                ),
                g.exp
            ),
            g.diamond,
            COALESCE(gc.daily_love_point_limit, 100),
            COALESCE(gc.daily_group_exp_limit, 200),
            g.created_at,
            g.buyer_user_id,
            g.seller_user_id,
            CURRENT_DATE
        FROM association_groups g
        LEFT JOIN group_configs gc ON gc.group_id = g.group_id
        WHERE g.group_id = $1
        "#,
    )
    .bind(gid)
    .fetch_optional(db)
    .await?;

    let (
        name,
        level,
        exp,
        next_level_exp,
        diamond,
        daily_limit,
        exp_limit,
        created_at,
        buyer_user_id,
        seller_user_id,
        today,
    ) = group_info.ok_or_else(|| CustomError::group_not_found("组不存在"))?;

    let tomorrow = today
        .succ_opt()
        .ok_or_else(|| CustomError::internal("看板日期超出支持范围"))?;
    let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
        .ok_or_else(|| CustomError::internal("无法计算本月起始日期"))?;

    // 每个指标都使用独立标量子查询，避免 orders × transactions × wishes 放大。
    let (today_metrics, month_metrics, quick_metrics, diamond_metrics, lp_balance) = tokio::try_join!(
        sqlx::query_as::<_, (i64, i64, i64)>(
            r#"
                SELECT
                    (
                        SELECT COUNT(*)::BIGINT
                        FROM orders o
                        WHERE o.group_id = $2
                          AND o.status IN (
                              'CONFIRMED_COMPLETED'::order_status_enum,
                              'COMPLETED'::order_status_enum
                          )
                          AND COALESCE(o.confirmed_at, o.completed_at, o.updated_at) >= $3::date
                          AND COALESCE(o.confirmed_at, o.completed_at, o.updated_at) < $4::date
                    ),
                    (
                        SELECT COALESCE(SUM(lt.amount), 0)::BIGINT
                        FROM love_point_transactions lt
                        WHERE lt.user_id = $1
                          AND lt.group_id = $2
                          AND lt.type = 'EARN'::love_point_tx_type_enum
                          AND lt.created_at >= $3::date
                          AND lt.created_at < $4::date
                    ),
                    (
                        SELECT COALESCE(SUM(gt.amount), 0)::BIGINT
                        FROM group_exp_transactions gt
                        WHERE gt.group_id = $2
                          AND gt.type = 'EARN'::group_exp_tx_type_enum
                          AND gt.created_at >= $3::date
                          AND gt.created_at < $4::date
                    )
                "#,
        )
        .bind(user_id)
        .bind(gid)
        .bind(today)
        .bind(tomorrow)
        .fetch_one(db),
        sqlx::query_as::<_, (i64, i64, i64, i64)>(
            r#"
                SELECT
                    (
                        SELECT COUNT(*)::BIGINT
                        FROM orders o
                        WHERE o.group_id = $2
                          AND o.status IN (
                              'CONFIRMED_COMPLETED'::order_status_enum,
                              'COMPLETED'::order_status_enum
                          )
                          AND COALESCE(o.confirmed_at, o.completed_at, o.updated_at) >= $3::date
                          AND COALESCE(o.confirmed_at, o.completed_at, o.updated_at) < $4::date
                    ),
                    (
                        SELECT COUNT(*)::BIGINT
                        FROM wishes w
                        WHERE w.group_id = $2
                          AND w.status = 'FINISHED'::wish_status_enum
                          AND COALESCE(w.finished_at, w.updated_at) >= $3::date
                          AND COALESCE(w.finished_at, w.updated_at) < $4::date
                    ),
                    (
                        SELECT COALESCE(SUM(lt.amount), 0)::BIGINT
                        FROM love_point_transactions lt
                        WHERE lt.user_id = $1
                          AND lt.group_id = $2
                          AND lt.type = 'DEDUCT'::love_point_tx_type_enum
                          AND lt.created_at >= $3::date
                          AND lt.created_at < $4::date
                    ),
                    (
                        SELECT COALESCE(SUM(lt.amount), 0)::BIGINT
                        FROM love_point_transactions lt
                        WHERE lt.user_id = $1
                          AND lt.group_id = $2
                          AND lt.type = 'EARN'::love_point_tx_type_enum
                          AND lt.created_at >= $3::date
                          AND lt.created_at < $4::date
                    )
                "#,
        )
        .bind(user_id)
        .bind(gid)
        .bind(month_start)
        .bind(tomorrow)
        .fetch_one(db),
        sqlx::query_as::<_, (i64, i64, i64, i32, i32, bool, i64)>(
            r#"
                SELECT
                    (SELECT COUNT(*)::BIGINT FROM orders o WHERE o.group_id = $1),
                    (SELECT COUNT(w.wish_id)::BIGINT FROM wishes w WHERE w.group_id = $1),
                    (
                        SELECT COUNT(*)::BIGINT
                        FROM wishes w
                        WHERE w.group_id = $1
                          AND w.status = 'FINISHED'::wish_status_enum
                    ),
                    COALESCE(
                        (
                            SELECT sr.consecutive_days
                            FROM sign_in_records sr
                            WHERE sr.group_id = $1
                              AND sr.user_id = $2
                              AND sr.sign_date = $5
                        ),
                        0
                    ),
                    COALESCE(
                        (
                            SELECT sr.consecutive_days
                            FROM sign_in_records sr
                            WHERE sr.group_id = $1
                              AND sr.user_id = $3
                              AND sr.sign_date = $5
                        ),
                        0
                    ),
                    EXISTS(
                        SELECT 1
                        FROM sign_in_records sr
                        WHERE sr.group_id = $1
                          AND sr.user_id = $4
                          AND sr.sign_date = $5
                    ),
                    (
                        SELECT COUNT(*)::BIGINT
                        FROM orders o
                        WHERE o.group_id = $1
                          AND o.status IN (
                              'CONFIRMED_COMPLETED'::order_status_enum,
                              'COMPLETED'::order_status_enum
                          )
                    )
                "#,
        )
        .bind(gid)
        .bind(buyer_user_id)
        .bind(seller_user_id)
        .bind(user_id)
        .bind(today)
        .fetch_one(db),
        sqlx::query_as::<_, (i64, i64)>(
            r#"
                SELECT
                    COALESCE(
                        SUM(dt.amount) FILTER (
                            WHERE dt.type = 'EARN'::diamond_tx_type_enum
                        ),
                        0
                    )::BIGINT,
                    COALESCE(
                        SUM(dt.amount) FILTER (
                            WHERE dt.type = 'CONSUME'::diamond_tx_type_enum
                        ),
                        0
                    )::BIGINT
                FROM diamond_transactions dt
                WHERE dt.group_id = $1
                "#,
        )
        .bind(gid)
        .fetch_one(db),
        sqlx::query_scalar::<_, i64>(
            r#"
                SELECT COALESCE(
                    (
                        SELECT ugp.available_love_point + ugp.frozen_love_point
                        FROM user_group_points ugp
                        WHERE ugp.user_id = $1 AND ugp.group_id = $2
                    ),
                    0
                )::BIGINT
                "#,
        )
        .bind(user_id)
        .bind(gid)
        .fetch_one(db),
    )?;

    let (orders_today, points_today, exp_today) = today_metrics;
    let (orders_month, wishes_finished, points_spent, points_earned) = month_metrics;
    let (
        total_orders,
        total_wishes,
        finished_wishes,
        user1_sign,
        user2_sign,
        signed_today,
        total_feeds,
    ) = quick_metrics;
    let (diamonds_earned, diamonds_spent) = diamond_metrics;

    let fulfillment_rate = if total_wishes > 0 {
        finished_wishes as f64 / total_wishes as f64
    } else {
        0.0
    };
    let sign_in_status = serde_json::json!({
        "signed_today": signed_today,
        "user1_consecutive_days": user1_sign,
        "user2_consecutive_days": user2_sign
    });
    let days_together = dashboard_i32(
        (today - created_at.date_naive()).num_days().max(0),
        "相遇天数",
    )?;

    Ok(ApiResponse::success(GroupDashboardResponse {
        group: GroupInfo {
            group_id: gid,
            name,
            level,
            exp,
            next_level_exp,
            diamond,
            daily_love_point_limit: daily_limit,
            daily_group_exp_limit: exp_limit,
            created_at: created_at.to_rfc3339(),
        },
        today: TodayStats {
            date: today.to_string(),
            orders_completed: dashboard_i32(orders_today, "今日完成订单数")?,
            love_points_earned: dashboard_i32(points_today, "今日获得爱心积分")?,
            group_exp_earned: dashboard_i32(exp_today, "今日获得组经验")?,
            sign_in: sign_in_status,
        },
        this_month: MonthStats {
            orders_completed: dashboard_i32(orders_month, "本月完成订单数")?,
            wishes_finished: dashboard_i32(wishes_finished, "本月完成心愿数")?,
            love_points_spent: dashboard_i32(points_spent, "本月消耗爱心积分")?,
            love_points_earned: dashboard_i32(points_earned, "本月获得爱心积分")?,
            feeds_completed: dashboard_i32(orders_month, "本月投喂数")?,
        },
        quick_stats: QuickStats {
            total_orders: dashboard_i32(total_orders, "累计订单数")?,
            total_wishes: dashboard_i32(total_wishes, "累计心愿数")?,
            finished_wishes: dashboard_i32(finished_wishes, "累计完成心愿数")?,
            fulfillment_rate,
            continuous_sign_in_days_user1: user1_sign,
            continuous_sign_in_days_user2: user2_sign,
            total_feeds: dashboard_i32(total_feeds, "累计投喂数")?,
            days_together,
            total_diamonds_earned: diamonds_earned,
            total_diamonds_spent: diamonds_spent,
            total_love_points_balance: lp_balance,
        },
    }))
}

/// 获取管理员运营看板
/// GET /api/admin/dashboard
/// FSD v2 16.2
#[utoipa::path(
    get,
    path = "/api/admin/dashboard",
    tag = "数据看板",
    params(AdminDashboardQuery),
    responses(
        (status = 200, description = "获取成功", body = AdminDashboardResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "需要管理员权限"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_admin_dashboard(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
    query: Query<AdminDashboardQuery>,
) -> Result<impl Responder, CustomError> {
    // AdminToken 已校验:必须是 admin_users 表中 ACTIVE 状态的记录
    // 不再信任 UserRole::Admin(组内业务角色,非平台管理员)
    let db = &state.db_pool;
    let query = query.into_inner();
    let current_date: NaiveDate = sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(db)
        .await?;
    let (range_start, range_end) = resolve_admin_date_range(&query, current_date)
        .map_err(|message| CustomError::BadRequest(message.into()))?;

    // 总用户数和总组数必须分别聚合，不能用无连接条件的笛卡尔积。
    let (total_users, total_groups) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM users").fetch_one(db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM association_groups")
            .fetch_one(db),
    )?;

    let (new_users_today, new_groups_today, active_guest_orders): (i64, i64, i64) = sqlx::query_as(
        r#"
            SELECT
                (
                    SELECT COUNT(*)::BIGINT
                    FROM users u
                    WHERE u.created_at >= $1::date
                      AND u.created_at < ($2::date + 1)
                ),
                (
                    SELECT COUNT(*)::BIGINT
                    FROM association_groups g
                    WHERE g.created_at >= $1::date
                      AND g.created_at < ($2::date + 1)
                ),
                (
                    SELECT COUNT(*)::BIGINT
                    FROM orders o
                    WHERE o.type = 'GUEST'::order_type_enum
                      AND o.created_at >= $1::date
                      AND o.created_at < ($2::date + 1)
                )
            "#,
    )
    .bind(range_start)
    .bind(range_end)
    .fetch_one(db)
    .await?;

    // DAU 口径：报表结束日这个数据库自然日内，event_log.user_id 有事件或
    // users.last_login_at 落在当天的去重用户；未指定日期时结束日为 CURRENT_DATE。
    let dau: i64 = sqlx::query_scalar(
        r#"
        WITH active_users AS (
            SELECT el.user_id
            FROM event_log el
            WHERE el.user_id IS NOT NULL
              AND el.created_at >= $1::date
              AND el.created_at < ($1::date + 1)
            UNION
            SELECT u.user_id
            FROM users u
            WHERE u.last_login_at >= $1::date
              AND u.last_login_at < ($1::date + 1)
        )
        SELECT COUNT(*)::BIGINT FROM active_users
        "#,
    )
    .bind(range_end)
    .fetch_one(db)
    .await?;

    let (orders_today, completed_today, guest_today, normal_today, avg_completion_hours): (
        i64,
        i64,
        i64,
        i64,
        f64,
    ) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*)::BIGINT,
            COUNT(*) FILTER (
                WHERE o.status IN (
                    'CONFIRMED_COMPLETED'::order_status_enum,
                    'COMPLETED'::order_status_enum
                )
            )::BIGINT,
            COUNT(*) FILTER (
                WHERE o.type = 'GUEST'::order_type_enum
            )::BIGINT,
            COUNT(*) FILTER (
                WHERE o.type = 'NORMAL'::order_type_enum
            )::BIGINT,
            COALESCE(
                AVG(
                    GREATEST(
                        EXTRACT(
                            EPOCH FROM (
                                COALESCE(o.confirmed_at, o.completed_at, o.updated_at)
                                - o.created_at
                            )
                        )::DOUBLE PRECISION / 3600.0,
                        0.0
                    )
                ) FILTER (
                    WHERE o.status IN (
                        'CONFIRMED_COMPLETED'::order_status_enum,
                        'COMPLETED'::order_status_enum
                    )
                ),
                0.0
            )::DOUBLE PRECISION
        FROM orders o
        WHERE o.created_at >= $1::date
          AND o.created_at < ($2::date + 1)
        "#,
    )
    .bind(range_start)
    .bind(range_end)
    .fetch_one(db)
    .await?;

    let completion_rate = if orders_today > 0 {
        completed_today as f64 / orders_today as f64
    } else {
        0.0
    };

    let (total_wishes, active_wishes, finished_wishes, expired_wishes, frozen_points): (
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        r#"
        SELECT
            COUNT(w.wish_id)::BIGINT,
            COUNT(*) FILTER (
                WHERE w.status IN (
                    'CREATED'::wish_status_enum,
                    'NEGOTIATING'::wish_status_enum,
                    'CLAIMED'::wish_status_enum
                )
            )::BIGINT,
            COUNT(*) FILTER (
                WHERE w.status = 'FINISHED'::wish_status_enum
            )::BIGINT,
            COUNT(*) FILTER (
                WHERE w.status = 'EXPIRED'::wish_status_enum
            )::BIGINT,
            COALESCE(
                SUM(
                    CASE
                        WHEN w.status = 'CLAIMED'::wish_status_enum
                        THEN COALESCE(w.final_cost, w.claim_cost, w.wish_cost)
                        ELSE 0
                    END
                ),
                0
            )::BIGINT
        FROM wishes w
        "#,
    )
    .fetch_one(db)
    .await?;

    // 三种账本分别聚合后再组合，避免流水表之间相乘。
    let (points_today, diamonds_spent, exp_today) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(SUM(lt.amount), 0)::BIGINT
            FROM love_point_transactions lt
            WHERE lt.type = 'EARN'::love_point_tx_type_enum
              AND lt.created_at >= $1::date
              AND lt.created_at < ($2::date + 1)
            "#,
        )
        .bind(range_start)
        .bind(range_end)
        .fetch_one(db),
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(SUM(dt.amount), 0)::BIGINT
            FROM diamond_transactions dt
            WHERE dt.type = 'CONSUME'::diamond_tx_type_enum
              AND dt.created_at >= $1::date
              AND dt.created_at < ($2::date + 1)
            "#,
        )
        .bind(range_start)
        .bind(range_end)
        .fetch_one(db),
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(SUM(gt.amount), 0)::BIGINT
            FROM group_exp_transactions gt
            WHERE gt.type = 'EARN'::group_exp_tx_type_enum
              AND gt.created_at >= $1::date
              AND gt.created_at < ($2::date + 1)
            "#,
        )
        .bind(range_start)
        .bind(range_end)
        .fetch_one(db),
    )?;

    let avg_points = if completed_today > 0 {
        points_today as f64 / completed_today as f64
    } else {
        0.0
    };

    let (pending_review, suspected_fraud, banned_today): (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (
                SELECT COUNT(*)::BIGINT
                FROM orders o
                WHERE o.risk_status = 'SUSPECT'::risk_status_enum
                  AND o.point_grant_status = 'PENDING_REVIEW'::point_grant_status_enum
            ),
            (
                SELECT COUNT(*)::BIGINT
                FROM orders o
                WHERE o.risk_status = 'BLOCKED'::risk_status_enum
            ),
            (
                SELECT COUNT(*)::BIGINT
                FROM users u
                WHERE u.status = 'BANNED'::user_status_enum
                  AND u.updated_at >= $1::date
                  AND u.updated_at < ($2::date + 1)
            )
        "#,
    )
    .bind(range_start)
    .bind(range_end)
    .fetch_one(db)
    .await?;

    let (sign_ins_today, full_team_sign, avg_consecutive_days): (i64, i64, f64) = sqlx::query_as(
        r#"
            SELECT
                COUNT(DISTINCT sr.user_id)::BIGINT,
                COUNT(*) FILTER (WHERE sr.full_team_bonus)::BIGINT,
                COALESCE(
                    AVG(sr.consecutive_days::DOUBLE PRECISION),
                    0.0
                )::DOUBLE PRECISION
            FROM sign_in_records sr
            WHERE sr.sign_date >= $1
              AND sr.sign_date <= $2
            "#,
    )
    .bind(range_start)
    .bind(range_end)
    .fetch_one(db)
    .await?;

    Ok(ApiResponse::success(AdminDashboardResponse {
        overview: OverviewStats {
            total_users,
            total_groups,
            dau,
            new_users_today,
            new_groups_today,
            active_guest_orders_today: active_guest_orders,
        },
        order_stats: OrderStats {
            total_orders_today: orders_today,
            completed_orders_today: completed_today,
            completion_rate,
            avg_completion_hours,
            guest_orders_today: guest_today,
            normal_orders_today: normal_today,
        },
        wish_stats: WishStats {
            total_wishes,
            active_wishes,
            finished_wishes,
            expired_wishes,
            total_frozen_points: frozen_points,
        },
        economy_stats: EconomyStats {
            total_love_points_issued_today: points_today,
            total_diamonds_spent_today: diamonds_spent,
            total_group_exp_earned_today: exp_today,
            avg_love_point_per_order: avg_points,
        },
        risk_stats: RiskStats {
            pending_review_orders: pending_review,
            suspected_fraud_orders: suspected_fraud,
            banned_users_today: banned_today,
        },
        sign_in_stats: SignInStats {
            total_sign_ins_today: sign_ins_today,
            full_team_sign_ins_today: full_team_sign,
            avg_consecutive_days,
        },
    }))
}

/// 获取趋势数据
/// GET /api/admin/dashboard/trends
/// FSD v2 16.3
#[utoipa::path(
    get,
    path = "/api/admin/dashboard/trends",
    tag = "数据看板",
    params(TrendsQuery),
    responses(
        (status = 200, description = "获取成功", body = TrendsResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "需要管理员权限"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_dashboard_trends(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
    query: Query<TrendsQuery>,
) -> Result<impl Responder, CustomError> {
    // AdminToken 已校验为 admin_users 表中的 ACTIVE 管理员
    let granularity = query.granularity.as_deref().unwrap_or("DAY");
    if granularity != "DAY" {
        return Err(CustomError::BadRequest("granularity 目前仅支持 DAY".into()));
    }

    let metric = TrendMetric::parse(&query.metric).ok_or_else(|| {
        CustomError::BadRequest("metric 必须是 DAU、ORDERS、WISHES、LOVE_POINTS 或 SIGN_INS".into())
    })?;
    let (start_date, end_date) = validate_dashboard_date_range(&query.start_date, &query.end_date)
        .map_err(|message| CustomError::BadRequest(message.into()))?;

    let sql = match metric {
        TrendMetric::Dau => {
            r#"
            WITH days AS (
                SELECT generate_series(
                    $1::date,
                    $2::date,
                    INTERVAL '1 day'
                )::date AS day
            ),
            activity AS (
                SELECT el.created_at::date AS day, el.user_id
                FROM event_log el
                WHERE el.user_id IS NOT NULL
                  AND el.created_at >= $1::date
                  AND el.created_at < ($2::date + 1)
                UNION
                SELECT u.last_login_at::date AS day, u.user_id
                FROM users u
                WHERE u.last_login_at >= $1::date
                  AND u.last_login_at < ($2::date + 1)
            ),
            daily AS (
                SELECT a.day, COUNT(DISTINCT a.user_id)::BIGINT AS value
                FROM activity a
                GROUP BY a.day
            )
            SELECT d.day, COALESCE(a.value, 0)::BIGINT
            FROM days d
            LEFT JOIN daily a ON a.day = d.day
            ORDER BY d.day
            "#
        }
        TrendMetric::Orders => {
            r#"
            WITH days AS (
                SELECT generate_series(
                    $1::date,
                    $2::date,
                    INTERVAL '1 day'
                )::date AS day
            ),
            daily AS (
                SELECT o.created_at::date AS day, COUNT(*)::BIGINT AS value
                FROM orders o
                WHERE o.created_at >= $1::date
                  AND o.created_at < ($2::date + 1)
                GROUP BY o.created_at::date
            )
            SELECT d.day, COALESCE(a.value, 0)::BIGINT
            FROM days d
            LEFT JOIN daily a ON a.day = d.day
            ORDER BY d.day
            "#
        }
        TrendMetric::Wishes => {
            r#"
            WITH days AS (
                SELECT generate_series(
                    $1::date,
                    $2::date,
                    INTERVAL '1 day'
                )::date AS day
            ),
            daily AS (
                SELECT w.created_at::date AS day, COUNT(w.wish_id)::BIGINT AS value
                FROM wishes w
                WHERE w.created_at >= $1::date
                  AND w.created_at < ($2::date + 1)
                GROUP BY w.created_at::date
            )
            SELECT d.day, COALESCE(a.value, 0)::BIGINT
            FROM days d
            LEFT JOIN daily a ON a.day = d.day
            ORDER BY d.day
            "#
        }
        TrendMetric::LovePoints => {
            r#"
            WITH days AS (
                SELECT generate_series(
                    $1::date,
                    $2::date,
                    INTERVAL '1 day'
                )::date AS day
            ),
            daily AS (
                SELECT
                    lt.created_at::date AS day,
                    COALESCE(SUM(lt.amount), 0)::BIGINT AS value
                FROM love_point_transactions lt
                WHERE lt.type = 'EARN'::love_point_tx_type_enum
                  AND lt.created_at >= $1::date
                  AND lt.created_at < ($2::date + 1)
                GROUP BY lt.created_at::date
            )
            SELECT d.day, COALESCE(a.value, 0)::BIGINT
            FROM days d
            LEFT JOIN daily a ON a.day = d.day
            ORDER BY d.day
            "#
        }
        TrendMetric::SignIns => {
            r#"
            WITH days AS (
                SELECT generate_series(
                    $1::date,
                    $2::date,
                    INTERVAL '1 day'
                )::date AS day
            ),
            daily AS (
                SELECT
                    sr.sign_date AS day,
                    COUNT(DISTINCT sr.user_id)::BIGINT AS value
                FROM sign_in_records sr
                WHERE sr.sign_date >= $1
                  AND sr.sign_date <= $2
                GROUP BY sr.sign_date
            )
            SELECT d.day, COALESCE(a.value, 0)::BIGINT
            FROM days d
            LEFT JOIN daily a ON a.day = d.day
            ORDER BY d.day
            "#
        }
    };

    let rows: Vec<(NaiveDate, i64)> = sqlx::query_as(sql)
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&state.db_pool)
        .await?;
    let data_points = rows
        .into_iter()
        .map(|(date, value)| DataPoint {
            date: date.to_string(),
            value,
        })
        .collect();

    Ok(ApiResponse::success(TrendsResponse {
        metric: metric.as_str().to_string(),
        granularity: "DAY".to_string(),
        data_points,
    }))
}

/// 获取组活动流 (过往足迹, bindDetail 页面用)
/// GET /api/groups/{group_id}/activities?cursor=...&limit=...
/// 鉴权: 必须是该组的 ACTIVE 成员
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/activities",
    tag = "数据看板",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        ("cursor" = Option<String>, Query, description = "上一页最后一条的 cursor, 第一页不传"),
        ("limit" = Option<i64>, Query, description = "每页条数, 默认 20, 最大 100"),
    ),
    responses(
        (status = 200, description = "获取成功", body = crate::domain::dashboard::GroupActivityListResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_group_activities(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    query: Query<crate::domain::dashboard::GroupActivityQuery>,
) -> Result<impl Responder, CustomError> {
    use crate::application::dashboard_service::DashboardService;
    let group_id = path.into_inner();
    let q = query.into_inner();
    let db = &state.db_pool;

    require_active_target_group_member(db, token.user_id, group_id).await?;

    let (events, next_cursor, has_more) =
        DashboardService::get_group_activities(db, group_id, &q).await?;

    Ok(ApiResponse::success(
        crate::domain::dashboard::GroupActivityListResponse {
            events,
            next_cursor,
            has_more,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trend_metric_accepts_all_public_values() {
        assert_eq!(TrendMetric::parse("DAU"), Some(TrendMetric::Dau));
        assert_eq!(TrendMetric::parse("ORDERS"), Some(TrendMetric::Orders));
        assert_eq!(TrendMetric::parse("WISHES"), Some(TrendMetric::Wishes));
        assert_eq!(
            TrendMetric::parse("LOVE_POINTS"),
            Some(TrendMetric::LovePoints)
        );
        assert_eq!(TrendMetric::parse("SIGN_INS"), Some(TrendMetric::SignIns));
    }

    #[test]
    fn trend_metric_rejects_unknown_or_noncanonical_values() {
        assert_eq!(TrendMetric::parse("USERS"), None);
        assert_eq!(TrendMetric::parse("orders"), None);
        assert_eq!(TrendMetric::parse(" ORDERS"), None);
    }

    #[test]
    fn dashboard_date_range_accepts_valid_inclusive_range() {
        let range = validate_dashboard_date_range("2026-02-28", "2026-03-01").unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
            )
        );
    }

    #[test]
    fn dashboard_date_range_rejects_bad_format_and_impossible_date() {
        assert!(validate_dashboard_date_range("2026-2-01", "2026-02-02").is_err());
        assert!(validate_dashboard_date_range("2026-02-30", "2026-03-01").is_err());
        assert!(validate_dashboard_date_range("2026/02/01", "2026-02-02").is_err());
    }

    #[test]
    fn dashboard_date_range_rejects_reversed_range() {
        assert!(validate_dashboard_date_range("2026-03-02", "2026-03-01").is_err());
    }

    #[test]
    fn dashboard_date_range_caps_inclusive_days() {
        assert!(validate_dashboard_date_range("2025-01-01", "2026-01-01").is_ok());
        assert!(validate_dashboard_date_range("2025-01-01", "2026-01-02").is_err());
    }
}
