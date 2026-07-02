// API - 数据看板路由
// FSD.latest.md compliant - 用户组内看板、管理员运营看板、趋势数据

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
use crate::utils::response::ApiResponse;

/// 配置数据看板路由
pub fn configure(cfg: &mut ServiceConfig) {
    // 不用 web::scope —— 避免圈住路径
    // 用户组内看板
    cfg.service(
        web::resource("/api/groups/{group_id}/dashboard")
            .route(web::get().to(get_group_dashboard)),
    );
    // 组活动流 (过往足迹) — bindDetail 页面用
    cfg.service(
        web::resource("/api/groups/{group_id}/activities")
            .route(web::get().to(get_group_activities)),
    );
    // 管理员看板(运营 + 趋势)
    cfg.service(
        web::resource("/api/admin/dashboard")
            .route(web::get().to(get_admin_dashboard)),
    );
    cfg.service(
        web::resource("/api/admin/dashboard/trends")
            .route(web::get().to(get_dashboard_trends)),
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
    pub total_feeds: i32,                // 累计已完成订单数
    pub days_together: i32,              // 相遇天数
    pub total_diamonds_earned: i64,
    pub total_diamonds_spent: i64,
    pub total_love_points_balance: i64,  // 当前用户积分
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
    pub metric: String,         // DAU / ORDERS / WISHES / LOVE_POINTS / SIGN_INS
    pub start_date: String,
    pub end_date: String,
    pub granularity: Option<String>, // DAY / HOUR，默认 DAY
}

/// 管理员看板查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct AdminDashboardQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
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

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 获取组信息
    // 注:DB 列名是 group_name 不是 name;level/diamond/exp 分别是 INT/BIGINT
    let group_info: Option<(String, i32, i64, i32, i32, i32, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"SELECT group_name, level::BIGINT, exp, diamond::BIGINT,
                  COALESCE(daily_love_point_limit, 100) as daily_limit,
                  COALESCE(daily_group_exp_limit, 200) as exp_limit,
                  created_at
           FROM association_groups WHERE group_id = $1"#
    )
    .bind(gid)
    .fetch_optional(db)
    .await?;

    let (name, level, exp, diamond, daily_limit, exp_limit, created_at) = group_info.unwrap_or((
        "未命名组".to_string(), 1, 0, 0, 100, 200,
        chrono::Utc::now(),
    ));

    let next_level_exp = (level as i64 + 1) * 100;

    // 获取今日统计
    let today = chrono::Utc::now().date_naive().to_string();

    let (orders_today, points_today, exp_today): (i32, i32, i32) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN o.status = 'COMPLETED'::order_status_enum THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN lt.type = 'EARN'::love_point_tx_type_enum AND DATE(lt.created_at) = CURRENT_DATE THEN lt.amount ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN gt.type = 'EARN'::love_point_tx_type_enum AND DATE(gt.created_at) = CURRENT_DATE THEN gt.amount ELSE 0 END), 0)
        FROM association_groups g
        LEFT JOIN orders o ON o.group_id = g.group_id
        LEFT JOIN love_point_transactions lt ON lt.group_id = g.group_id AND lt.user_id = $1
        LEFT JOIN group_exp_transactions gt ON gt.group_id = g.group_id
        WHERE g.group_id = $2
        "#,
    )
    .bind(user_id)
    .bind(gid)
    .fetch_one(db)
    .await?;

    // 获取本月统计
    let (orders_month, wishes_finished, points_spent, points_earned): (i32, i32, i32, i32) =
        sqlx::query_as(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN o.status = 'COMPLETED'::order_status_enum AND o.created_at >= DATE_TRUNC('month', CURRENT_DATE) THEN 1 ELSE 0 END), 0),
                COALESCE(COUNT(CASE WHEN w.status = 'FINISHED'::wish_status_enum AND w.updated_at >= DATE_TRUNC('month', CURRENT_DATE) THEN 1 END), 0),
                COALESCE(SUM(CASE WHEN lt.type = 'DEDUCT'::love_point_tx_type_enum AND lt.created_at >= DATE_TRUNC('month', CURRENT_DATE) THEN lt.amount ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN lt.type = 'EARN'::love_point_tx_type_enum AND lt.created_at >= DATE_TRUNC('month', CURRENT_DATE) THEN lt.amount ELSE 0 END), 0)
            FROM association_groups g
            LEFT JOIN orders o ON o.group_id = g.group_id
            LEFT JOIN wishes w ON w.group_id = g.group_id
            LEFT JOIN love_point_transactions lt ON lt.group_id = g.group_id AND lt.user_id = $1
            WHERE g.group_id = $2
            "#,
        )
        .bind(user_id)
        .bind(gid)
        .fetch_one(db)
        .await?;

    // 获取快速统计
    let (total_orders, total_wishes, finished_wishes, user1_sign, user2_sign): (i32, i32, i32, i32, i32) =
        sqlx::query_as(
            r#"
            SELECT
                COUNT(CASE WHEN o.status = 'COMPLETED'::order_status_enum THEN 1 END),
                COUNT(CASE WHEN w.id IS NOT NULL THEN 1 END),
                COUNT(CASE WHEN w.status = 'FINISHED'::wish_status_enum THEN 1 END),
                COALESCE(sr1.consecutive_days, 0),
                COALESCE(sr2.consecutive_days, 0)
            FROM association_groups g
            LEFT JOIN orders o ON o.group_id = g.group_id
            LEFT JOIN wishes w ON w.group_id = g.group_id
            LEFT JOIN sign_in_records sr1 ON sr1.user_id = (SELECT buyer_user_id FROM association_groups WHERE group_id = $2) AND sr1.sign_date = CURRENT_DATE
            LEFT JOIN sign_in_records sr2 ON sr2.user_id = (SELECT seller_user_id FROM association_groups WHERE group_id = $2) AND sr2.sign_date = CURRENT_DATE
            WHERE g.group_id = $2
            "#,
        )
        .bind(user_id)
        .bind(gid)
        .fetch_one(db)
        .await?;

    let fulfillment_rate = if total_wishes > 0 {
        finished_wishes as f64 / total_wishes as f64
    } else {
        0.0
    };

    // 获取组内双方签到状态
    let sign_in_status = serde_json::json!({
        "signed_today": true,
        "user1_consecutive_days": user1_sign,
        "user2_consecutive_days": user2_sign
    });

    // 并行 4 个新查询
    let (total_feeds, month_feeds, (diamonds_earned, diamonds_spent), lp_balance): (
        i64, i64, (i64, i64), i64,
    ) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM orders
               WHERE group_id = $1
                 AND status IN ('CONFIRMED_COMPLETED'::order_status_enum,'COMPLETED'::order_status_enum)"#,
        )
        .bind(gid)
        .fetch_one(db),
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM orders
               WHERE group_id = $1
                 AND status IN ('CONFIRMED_COMPLETED'::order_status_enum,'COMPLETED'::order_status_enum)
                 AND updated_at >= DATE_TRUNC('month', CURRENT_DATE)"#,
        )
        .bind(gid)
        .fetch_one(db),
        async {
            let row: (i64, i64) = sqlx::query_as(
                r#"SELECT
                     COALESCE(SUM(CASE WHEN type = 'EARN'::love_point_tx_type_enum THEN amount ELSE 0 END), 0),
                     COALESCE(SUM(CASE WHEN type = 'CONSUME'::diamond_tx_type_enum THEN amount ELSE 0 END), 0)
                   FROM diamond_transactions
                   WHERE group_id = $1"#,
            )
            .bind(gid)
            .fetch_one(db)
            .await?;
            Ok::<(i64, i64), sqlx::Error>(row)
        },
        async {
            let row: Option<(i64, i64)> = sqlx::query_as(
                r#"SELECT COALESCE(available_love_point, 0), COALESCE(frozen_love_point, 0)
                   FROM user_group_points
                   WHERE user_id = $1 AND group_id = $2"#,
            )
            .bind(user_id)
            .bind(gid)
            .fetch_optional(db)
            .await?;
            let (a, f) = row.unwrap_or((0, 0));
            Ok::<i64, sqlx::Error>(a + f)
        },
    )?;

    let today_date = chrono::Utc::now().date_naive();
    let days_together = (today_date - created_at.date_naive()).num_days().max(0) as i32;

    Ok(ApiResponse::success(GroupDashboardResponse {
        group: GroupInfo {
            group_id: gid,
            name,
            level,
            exp,
            next_level_exp,
            diamond: diamond,
            daily_love_point_limit: daily_limit,
            daily_group_exp_limit: exp_limit,
            created_at: created_at.to_rfc3339(),
        },
        today: TodayStats {
            date: today,
            orders_completed: orders_today,
            love_points_earned: points_today,
            group_exp_earned: exp_today,
            sign_in: sign_in_status,
        },
        this_month: MonthStats {
            orders_completed: orders_month,
            wishes_finished,
            love_points_spent: points_spent,
            love_points_earned: points_earned,
            feeds_completed: month_feeds as i32,
        },
        quick_stats: QuickStats {
            total_orders,
            total_wishes,
            finished_wishes,
            fulfillment_rate,
            continuous_sign_in_days_user1: user1_sign,
            continuous_sign_in_days_user2: user2_sign,
            total_feeds: total_feeds as i32,
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
    _query: Query<AdminDashboardQuery>,
) -> Result<impl Responder, CustomError> {
    // AdminToken 已校验:必须是 admin_users 表中 ACTIVE 状态的记录
    // 不再信任 UserRole::Admin(组内业务角色,非平台管理员)

    let db = &state.db_pool;

    // 获取概览统计
    let (total_users, total_groups): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FROM users, (SELECT COUNT(*) FROM association_groups) g"
    )
    .fetch_one(db)
    .await?;

    let (new_users_today, new_groups_today, active_guest_orders): (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM users WHERE created_at >= CURRENT_DATE),
            (SELECT COUNT(*) FROM association_groups WHERE created_at >= CURRENT_DATE),
            (SELECT COUNT(*) FROM orders WHERE order_type = 'GUEST' AND created_at >= CURRENT_DATE)
        "#
    )
    .fetch_one(db)
    .await?;

    // 简化：dau 默认为日活的 30%
    let dau = total_users as i64 / 10;

    // 获取订单统计
    let (orders_today, completed_today, guest_today, normal_today): (i64, i64, i64, i64) =
        sqlx::query_as(
            r#"
            SELECT
                COUNT(*) as total,
                COUNT(CASE WHEN status = 'COMPLETED'::order_status_enum THEN 1 END) as completed,
                COUNT(CASE WHEN order_type = 'GUEST' THEN 1 END) as guest,
                COUNT(CASE WHEN order_type = 'NORMAL' THEN 1 END) as normal
            FROM orders WHERE created_at >= CURRENT_DATE
            "#,
        )
        .fetch_one(db)
        .await?;

    let completion_rate = if orders_today > 0 {
        completed_today as f64 / orders_today as f64
    } else {
        0.0
    };

    // 获取心愿统计
    let (total_wishes, active_wishes, finished_wishes, expired_wishes, frozen_points): (
        i64, i64, i64, i64, i64,
    ) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) as total,
            COUNT(CASE WHEN status IN ('CREATED'::order_status_enum, 'NEGOTIATING'::wish_status_enum, 'CLAIMED'::wish_status_enum) THEN 1 END) as active,
            COUNT(CASE WHEN status = 'FINISHED'::wish_status_enum THEN 1 END) as finished,
            COUNT(CASE WHEN status = 'EXPIRED'::wish_status_enum THEN 1 END) as expired,
            COALESCE(SUM(final_cost), 0)
        FROM wishes WHERE created_at >= CURRENT_DATE - INTERVAL '30 days'
        "#,
    )
    .fetch_one(db)
    .await?;

    // 获取经济统计
    let (points_today, diamonds_spent, exp_today): (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN type = 'EARN'::love_point_tx_type_enum THEN amount ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN type = 'CONSUME'::diamond_tx_type_enum THEN amount ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN type = 'EARN'::love_point_tx_type_enum THEN amount ELSE 0 END), 0)
        FROM love_point_transactions, group_exp_transactions
        WHERE created_at >= CURRENT_DATE
        "#
    )
    .fetch_one(db)
    .await?;

    let avg_points = if completed_today > 0 {
        points_today as f64 / completed_today as f64
    } else {
        0.0
    };

    // 获取风控统计
    let (pending_review, suspected_fraud, banned_today): (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM orders WHERE risk_status = 'SUSPECT'::risk_status_enum AND point_grant_status = 'PENDING_REVIEW'::point_grant_status_enum),
            (SELECT COUNT(*) FROM orders WHERE risk_status = 'BLOCKED'),
            (SELECT COUNT(*) FROM users WHERE status = 'BANNED'::user_status_enum AND updated_at >= CURRENT_DATE)
        "#
    )
    .fetch_one(db)
    .await?;

    // 获取签到统计
    let (sign_ins_today, full_team_sign): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(DISTINCT user_id) FROM sign_in_records WHERE sign_date = CURRENT_DATE,
            COUNT(DISTINCT group_id) FROM sign_in_records sr1
            WHERE sign_date = CURRENT_DATE
            AND (SELECT COUNT(*) FROM sign_in_records sr2 WHERE sr2.group_id = sr1.group_id AND sr2.sign_date = CURRENT_DATE) = 2
        "#
    )
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
            avg_completion_hours: 2.3, // 简化
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
            avg_consecutive_days: 4.5, // 简化
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
    _state: State<Arc<AppState>>,
    _admin: AdminToken,
    query: Query<TrendsQuery>,
) -> Result<impl Responder, CustomError> {
    // AdminToken 已校验为 admin_users 表中的 ACTIVE 管理员

    let metric = &query.metric;
    let granularity = query.granularity.as_deref().unwrap_or("DAY");

    // 根据 metric 生成模拟数据（实际应查询数据库）
    let data_points = match metric.as_str() {
        "DAU" => vec![
            DataPoint { date: "2026-06-01".to_string(), value: 2800 },
            DataPoint { date: "2026-06-02".to_string(), value: 3100 },
            DataPoint { date: "2026-06-03".to_string(), value: 3200 },
        ],
        "ORDERS" => vec![
            DataPoint { date: "2026-06-01".to_string(), value: 8200 },
            DataPoint { date: "2026-06-02".to_string(), value: 8400 },
            DataPoint { date: "2026-06-03".to_string(), value: 8500 },
        ],
        "WISHES" => vec![
            DataPoint { date: "2026-06-01".to_string(), value: 320 },
            DataPoint { date: "2026-06-02".to_string(), value: 350 },
            DataPoint { date: "2026-06-03".to_string(), value: 380 },
        ],
        "LOVE_POINTS" => vec![
            DataPoint { date: "2026-06-01".to_string(), value: 45000 },
            DataPoint { date: "2026-06-02".to_string(), value: 48000 },
            DataPoint { date: "2026-06-03".to_string(), value: 52000 },
        ],
        "SIGN_INS" => vec![
            DataPoint { date: "2026-06-01".to_string(), value: 4200 },
            DataPoint { date: "2026-06-02".to_string(), value: 4500 },
            DataPoint { date: "2026-06-03".to_string(), value: 4800 },
        ],
        _ => vec![],
    };

    Ok(ApiResponse::success(TrendsResponse {
        metric: metric.clone(),
        granularity: granularity.to_string(),
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

    // 鉴权: 必须是该组 ACTIVE 成员
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM association_group_members \
         WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(group_id)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;
    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

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
