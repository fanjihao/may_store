// API 层 - 后台管理路由
// 处理运营配置、数据统计、权限管理等后台管理功能

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::api::admin::auth;
use crate::api::admin::footprint_groups;
use crate::api::admin::group_levels;
use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::admin_auth::AdminToken;
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use crate::utils::response::ApiResponse;

/// Single config whitelist + range + category
const CONFIG_ENTRIES: &[(&str, i64, i64, &str)] = &[
    ("orderPointPercent", 1, 200, "ORDER"),
    ("diamondUnlockCost", 10, 10000, "REWARDS"),
    ("defaultFootprintCapacity", 10, 1000, "GENERAL"),
    ("footprintExpandDiamondCost", 1, 1000, "REWARDS"),
    ("orderCompleteExp", 0, 10000, "ORDER"),
    // 2026-07-06 新增: 每日奖励上限 (基础值 + 等级增量)
    ("dailyGroupExpLimit", 0, 100000, "ORDER"),
    ("dailyGroupExpLimitLevelStep", 0, 10000, "ORDER"),
    ("dailyLovePointLimit", 0, 100000, "ORDER"),
    ("dailyLovePointLimitLevelStep", 0, 10000, "ORDER"),
    ("fullTeamBonusAmt", 0, 100, "SIGN_IN"),
    ("confirmedFinishedPoints", -1000, 1000, "ORDER"),
    ("confirmedUnfinishedPoints", -1000, 1000, "ORDER"),
    ("breederClosedPoints", -1000, 1000, "ORDER"),
    ("timeoutPoints", -1000, 1000, "ORDER"),
];

const SIGN_IN_REWARDS_ELEMENT_MIN: i64 = 1;
const SIGN_IN_REWARDS_ELEMENT_MAX: i64 = 100;
const SIGN_IN_REWARDS_REQUIRED_LEN: usize = 7;

/// Validate a single config value
///
/// - Integer keys: must be within [min, max]
/// - signInRewards7Days: must be a 7-element array, each element 1-100
/// - Unknown key: BadRequest
pub fn validate_config(
    key: &str,
    value: &serde_json::Value,
) -> Result<(), CustomError> {
    if key == "signInRewards7Days" {
        let arr = value.as_array().ok_or_else(|| {
            CustomError::BadRequest("signInRewards7Days 必须是数组".into())
        })?;
        if arr.len() != SIGN_IN_REWARDS_REQUIRED_LEN {
            return Err(CustomError::BadRequest(format!(
                "signInRewards7Days 必须正好 {} 个元素,当前 {} 个",
                SIGN_IN_REWARDS_REQUIRED_LEN,
                arr.len()
            )));
        }
        for v in arr {
            let n = v.as_i64().ok_or_else(|| {
                CustomError::BadRequest("signInRewards7Days 元素必须是整数".into())
            })?;
            if !(SIGN_IN_REWARDS_ELEMENT_MIN..=SIGN_IN_REWARDS_ELEMENT_MAX).contains(&n) {
                return Err(CustomError::BadRequest(format!(
                    "signInRewards7Days 元素必须在 {}-{} 之间,当前 {}",
                    SIGN_IN_REWARDS_ELEMENT_MIN, SIGN_IN_REWARDS_ELEMENT_MAX, n
                )));
            }
        }
        return Ok(());
    }

    let (_, lo, hi, _cat) = CONFIG_ENTRIES
        .iter()
        .find(|(k, _, _, _)| *k == key)
        .ok_or_else(|| CustomError::BadRequest(format!("未知配置键: {}", key)))?;
    let v = value
        .as_i64()
        .ok_or_else(|| CustomError::BadRequest("value 必须是整数".into()))?;
    if v < *lo || v > *hi {
        return Err(CustomError::BadRequest(format!(
            "{} 必须在 {}-{} 之间",
            key, lo, hi
        )));
    }
    Ok(())
}

/// 配置后台管理路由
pub fn configure(cfg: &mut ServiceConfig) {
    // auth 子模块在独立 scope /api/admin/auth/login —— 不冲突,先注册
    auth::configure(cfg);
    // 组等级配置 (独立的 scope, 跟 /groups 平行)
    group_levels::configure(cfg);
    // 足迹分组管理 (独立的 scope) - multi-admin 维护公用足迹分组
    footprint_groups::configure(cfg);

    // 所有路由放主 scope /api/admin 下,避免 sub-scope shadow 父 scope 的 bare GET
    // (T2/T6 教训:把 PATCH /users/{user_id} 放独立 sub-scope 会让 GET /users 返 404)
    cfg.service(
        web::scope("/api/admin")
            .route("/stats", web::get().to(get_stats))
            .route("/groups", web::get().to(list_groups))
            .route("/users", web::get().to(list_all_users))
            .route("/configs", web::get().to(get_config))
            .route("/configs/{config_key}", web::patch().to(update_config))
            // 用户管理 (T2)
            .route("/users/{user_id}", web::patch().to(crate::api::admin::users::update_user))
            // 双人组管理 (T4/T5)
            .route("/groups/{group_id}", web::patch().to(crate::api::admin::groups::update_group))
            .route(
                "/groups/{group_id}/members",
                web::get().to(crate::api::admin::groups::get_group_members),
            )
            // FSD v2: 心愿质量奖励审核
            .route(
                "/wishes/{wish_id}/quality-reward",
                web::post().to(wish_quality_reward),
            )
            // FSD v2: 订单积分/经验审核
            .route(
                "/orders/{order_id}/reward-review",
                web::post().to(order_reward_review),
            )
            // FSD v2: 获取待审核订单列表
            .route("/orders/pending-review", web::get().to(get_pending_review_orders))
            // FSD v2: 审核风险订单
            .route("/orders/{order_id}/review", web::post().to(review_order))
            // FSD v2: 获取审计日志
            .route("/audit-logs", web::get().to(get_audit_logs))
            // FSD v2: 组级配置更新
            .route("/groups/{group_id}/configs", web::patch().to(update_group_configs))
            // FSD v2: 积分补偿
            .route(
                "/groups/{group_id}/points/compensate",
                web::post().to(compensate_points),
            )
            // FSD v2: 钻石补偿
            .route(
                "/groups/{group_id}/diamonds/compensate",
                web::post().to(compensate_diamonds),
            )
            // FSD §24.7: 菜品审核
            .route("/foods/pending", web::get().to(list_pending_food_audits))
            .route("/foods/{food_id}/audit", web::post().to(audit_food)),
    );
}

// ============== 响应结构体 ==============

/// 系统统计响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub total_users: i64,
    pub total_groups: i64,
    pub total_orders: i64,
    pub active_orders: i64,
    pub total_diamonds: i64,
}

/// 组列表项
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupListItem {
    pub group_id: i64,
    pub group_name: String,
    pub diamond: i32,
    pub member_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 用户列表项
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserListItem {
    pub user_id: i64,
    pub username: String,
    pub nick_name: Option<String>,
    pub role: String,
    pub love_point: i32,
    pub diamond: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 系统配置响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    /// 7 天轮回签到奖励配置(数组下标对应 1~7 天)
    pub sign_in_rewards_7_days: Vec<i32>,
    pub order_point_percent: i32,
    pub diamond_unlock_cost: i32,
    pub default_footprint_capacity: i32,
    /// 足迹扩容每格的钻石单价 (扩 N 格 = N * 该值)
    pub footprint_expand_diamond_cost: i32,
    /// 订单确认完成后奖励的组经验值 (orders.exp_grant_status 防重发)
    pub order_complete_exp: i32,
    /// 每日组经验获取上限基础值 (实际 = base + level * level_step)
    pub daily_group_exp_limit: i32,
    /// 每日组经验上限每级增量
    pub daily_group_exp_limit_level_step: i32,
    /// 每日爱心积分获取上限基础值 (实际 = base + level * level_step)
    pub daily_love_point_limit: i32,
    /// 每日爱心积分上限每级增量
    pub daily_love_point_limit_level_step: i32,
    /// 全组满签时最后签到用户获得的组钻石数
    pub full_team_bonus_amt: i32,
    /// 订单确认完成后奖励的爱心积分（可正可负）
    pub confirmed_finished_points: i32,
    /// 订单确认未完成扣减的爱心积分
    pub confirmed_unfinished_points: i32,
    /// 主人家取消订单扣减的爱心积分
    pub breeder_closed_points: i32,
    /// 订单超时扣减的爱心积分
    pub timeout_points: i32,
}

/// 心愿质量奖励响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishQualityRewardResponse {
    pub wish_id: i64,
    pub quality_level: String,
    pub diamond_reward: i32,
    pub status: String,
}

/// 订单奖励审核响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderRewardReviewResponse {
    pub order_id: i64,
    pub point_grant_status_updated: bool,
    pub exp_grant_status_updated: bool,
    pub status: String,
}

/// 获取系统统计数据
/// 管理员查看整体运营数据
#[utoipa::path(
    get,
    path = "/api/admin/stats",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功", body = StatsResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_stats(
    state: State<Arc<AppState>>,
    admin: AdminToken,
) -> Result<impl Responder, CustomError> {
    let _ = admin; // AdminToken 已在 FromRequest 阶段校验通过
    let db = &state.db_pool;

    // 获取各项统计数据
    let total_users: i64 = sqlx::query("SELECT COUNT(*) FROM users")
        .fetch_one(db)
        .await?
        .get(0);

    let total_groups: i64 = sqlx::query("SELECT COUNT(*) FROM association_groups")
        .fetch_one(db)
        .await?
        .get(0);

    let total_orders: i64 = sqlx::query("SELECT COUNT(*) FROM orders")
        .fetch_one(db)
        .await?
        .get(0);

    let active_orders: i64 = sqlx::query(
        "SELECT COUNT(*) FROM orders WHERE status NOT IN ('COMPLETED', 'CANCELLED', 'REJECTED')",
    )
    .fetch_one(db)
    .await?
    .get(0);

    let total_diamonds: i64 =
        sqlx::query("SELECT COALESCE(SUM(diamond), 0) FROM association_groups")
            .fetch_one(db)
            .await?
            .get(0);

    let stats = serde_json::json!({
        "totalUsers": total_users,
        "totalGroups": total_groups,
        "totalOrders": total_orders,
        "activeOrders": active_orders,
        "totalDiamonds": total_diamonds
    });

    Ok(ApiResponse::success(stats))
}

/// 获取所有组列表
#[utoipa::path(
    get,
    path = "/api/admin/groups",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功", body = Vec<GroupListItem>),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_groups(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
) -> Result<impl Responder, CustomError> {
    let groups = sqlx::query(
        "SELECT group_id, group_name, diamond, member_count, created_at FROM association_groups ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&state.db_pool)
    .await?;

    let result: Vec<serde_json::Value> = groups
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "groupId": r.get::<i64, _>("group_id"),
                "groupName": r.get::<String, _>("group_name"),
                "diamond": r.get::<i32, _>("diamond"),
                "memberCount": r.get::<i32, _>("member_count"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();

    Ok(ApiResponse::success(result))
}

/// 获取所有用户列表
#[utoipa::path(
    get,
    path = "/api/admin/users",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功", body = Vec<UserListItem>),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_all_users(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
) -> Result<impl Responder, CustomError> {
    let users = sqlx::query(
        "SELECT user_id, username, nick_name, role::text, love_point, diamond, created_at FROM users ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&state.db_pool)
    .await?;

    let result: Vec<serde_json::Value> = users
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "userId": r.get::<i64, _>("user_id"),
                "username": r.get::<String, _>("username"),
                "nickName": r.get::<Option<String>, _>("nick_name"),
                "role": r.get::<String, _>("role"),
                "lovePoint": r.get::<i32, _>("love_point"),
                "diamond": r.get::<i32, _>("diamond"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();

    Ok(ApiResponse::success(result))
}

/// 单条系统配置更新输入 (FSD §11.22 PATCH 接口)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigInput {
    /// 新值,根据 config_key 类型自动校验
    pub value: serde_json::Value,
}

/// 获取系统配置列表 (FSD §11.22)
#[utoipa::path(
    get,
    path = "/api/admin/configs",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功", body = ConfigResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_config(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    // 默认值（DB 无记录时兜底）
    let default_rewards: Vec<i32> = vec![5, 6, 7, 8, 9, 10, 20];
    let default_order_point_percent: i32 = 100;
    let default_diamond_unlock_cost: i32 = 100;
    let default_footprint_capacity: i32 = 50;
    let default_footprint_expand_diamond_cost: i32 = 5;
    let default_order_complete_exp: i32 = 10;
    // 2026-07-06 新增: 每日奖励上限默认值
    let default_daily_group_exp_limit: i32 = 200;
    let default_daily_group_exp_limit_level_step: i32 = 20;
    let default_daily_love_point_limit: i32 = 100;
    let default_daily_love_point_limit_level_step: i32 = 10;
    let default_full_team_bonus_amt: i32 = 10;
    let default_confirmed_finished_points: i32 = 10;
    let default_confirmed_unfinished_points: i32 = -5;
    let default_breeder_closed_points: i32 = -8;
    let default_timeout_points: i32 = -3;

    // 读 7 天奖励数组
    let rewards: Vec<i32> = sqlx::query_as::<_, (Option<serde_json::Value>,)>(
        "SELECT config_value FROM global_configs WHERE config_key = $1",
    )
    .bind("signInRewards7Days")
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .and_then(|(v,)| v)
    .and_then(|v| serde_json::from_value::<Vec<i32>>(v).ok())
    .unwrap_or(default_rewards);

    // 读 4 个整数配置（共享 helper 闭包）
    async fn read_int(
        db: &sqlx::PgPool,
        key: &str,
        default: i32,
    ) -> i32 {
        sqlx::query_as::<_, (Option<serde_json::Value>,)>(
            "SELECT config_value FROM global_configs WHERE config_key = $1",
        )
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .and_then(|(v,)| v)
        .and_then(|v| v.as_i64().map(|n| n as i32))
        .unwrap_or(default)
    }

    let response = ConfigResponse {
        sign_in_rewards_7_days: rewards,
        order_point_percent: read_int(db, "orderPointPercent", default_order_point_percent).await,
        diamond_unlock_cost: read_int(db, "diamondUnlockCost", default_diamond_unlock_cost).await,
        default_footprint_capacity: read_int(db, "defaultFootprintCapacity", default_footprint_capacity).await,
        footprint_expand_diamond_cost: read_int(db, "footprintExpandDiamondCost", default_footprint_expand_diamond_cost).await,
        order_complete_exp: read_int(db, "orderCompleteExp", default_order_complete_exp).await,
        daily_group_exp_limit: read_int(db, "dailyGroupExpLimit", default_daily_group_exp_limit).await,
        daily_group_exp_limit_level_step: read_int(db, "dailyGroupExpLimitLevelStep", default_daily_group_exp_limit_level_step).await,
        daily_love_point_limit: read_int(db, "dailyLovePointLimit", default_daily_love_point_limit).await,
        daily_love_point_limit_level_step: read_int(db, "dailyLovePointLimitLevelStep", default_daily_love_point_limit_level_step).await,
        full_team_bonus_amt: read_int(db, "fullTeamBonusAmt", default_full_team_bonus_amt).await,
        confirmed_finished_points: read_int(db, "confirmedFinishedPoints", default_confirmed_finished_points).await,
        confirmed_unfinished_points: read_int(db, "confirmedUnfinishedPoints", default_confirmed_unfinished_points).await,
        breeder_closed_points: read_int(db, "breederClosedPoints", default_breeder_closed_points).await,
        timeout_points: read_int(db, "timeoutPoints", default_timeout_points).await,
    };

    Ok(ApiResponse::success(response))
}

/// 更新单条系统配置 (FSD §11.22 - PATCH /api/admin/configs/{config_key})
#[utoipa::path(
    patch,
    path = "/api/admin/configs/{config_key}",
    tag = "后台管理",
    params(("config_key" = String, Path, description = "配置键名,如 signInRewards7Days")),
    request_body = UpdateConfigInput,
    responses(
        (status = 200, description = "更新成功", body = serde_json::Value),
        (status = 400, description = "未知配置键或取值越界"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_config(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<String>,
    body: Json<UpdateConfigInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let config_key = path.into_inner();
    let new_value = body.into_inner().value;

    validate_config(&config_key, &new_value)?;

    let category = if config_key == "signInRewards7Days" {
        "SIGN_IN"
    } else {
        let (_, _, _, cat) = CONFIG_ENTRIES
            .iter()
            .find(|(k, _, _, _)| *k == config_key.as_str())
            .expect("validate_config 已确保 key 存在");
        *cat
    };

    sqlx::query(
        r#"INSERT INTO global_configs (config_key, config_value, category, updated_by, updated_at)
           VALUES ($1, $2::jsonb, $3::config_category_enum, $4, NOW())
           ON CONFLICT (config_key) DO UPDATE
           SET config_value = EXCLUDED.config_value,
               updated_by = EXCLUDED.updated_by,
               updated_at = NOW()"#,
    )
    .bind(&config_key)
    .bind(&new_value)
    .bind(category)
    .bind(admin.user_id)
    .execute(db)
    .await?;

    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, detail)
           VALUES ($1, 'ADMIN', 'CONFIG_UPDATE', 'GLOBAL_CONFIG', $2)"#,
    )
    .bind(admin.user_id)
    .bind(serde_json::json!({ "config_key": config_key, "value": new_value }))
    .execute(db)
    .await;

    Ok(ApiResponse::success(serde_json::json!({
        "config_key": config_key,
        "value": new_value,
        "status": "ok"
    })))
}

/// 心愿质量奖励审核
/// POST /api/admin/wishes/{wish_id}/quality-reward
///
/// 管理员查看打卡反馈后，根据质量发放额外组钻石奖励
/// 质量等级: NONE/NORMAL/GOOD/EXCELLENT
/// 必须幂等，同一心愿额外钻石奖励只发放一次
#[utoipa::path(
    post,
    path = "/api/admin/wishes/{wish_id}/quality-reward",
    tag = "后台管理",
    params(
        ("wish_id" = i64, description = "心愿ID")
    ),
    request_body = WishQualityRewardInput,
    responses(
        (status = 200, description = "奖励成功", body = WishQualityRewardResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "心愿不存在"),
        (status = 409, description = "已发放过奖励")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_quality_reward(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
    body: Json<WishQualityRewardInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = path.into_inner();
    let input = body.into_inner();

    let db = &state.db_pool;

    // 检查心愿是否存在且处于FINISHED状态
    let wish: Option<(String, i64)> =
        sqlx::query_as("SELECT status::text, group_id FROM wishes WHERE wish_id = $1")
            .bind(wish_id)
            .fetch_optional(db)
            .await?;

    let (status, group_id) = match wish {
        Some((s, g)) => (s, g),
        None => return Err(CustomError::NotFound("心愿不存在".into())),
    };

    if status != "FINISHED" {
        return Err(CustomError::BadRequest("心愿状态不允许质量奖励".into()));
    }

    // 检查是否已经发放过奖励 (幂等)
    let existing_reward: Option<(Option<i32>,)> = sqlx::query_as(
        "SELECT diamond_reward FROM wishes WHERE wish_id = $1 AND diamond_reward > 0",
    )
    .bind(wish_id)
    .fetch_optional(db)
    .await?;

    if existing_reward.is_some() {
        return Err(CustomError::Conflict("该心愿已发放过质量奖励".into()));
    }

    // 计算钻石奖励金额 (简化实现)
    let diamond_amount = match input.quality_level.as_str() {
        "NONE" => 0,
        "NORMAL" => 5,
        "GOOD" => 10,
        "EXCELLENT" => 20,
        _ => return Err(CustomError::BadRequest("无效的质量等级".into())),
    };

    // 更新心愿质量奖励记录
    sqlx::query(
        r#"
        UPDATE wishes SET
            quality_review_status = 'REVIEWED'::wish_quality_status_enum,
            quality_reviewer_id = $1,
            quality_remark = $2,
            diamond_reward = $3,
            updated_at = NOW()
        WHERE wish_id = $4
        "#,
    )
    .bind(admin.user_id)
    .bind(&input.remark)
    .bind(diamond_amount)
    .bind(wish_id)
    .execute(db)
    .await?;

    // 如果有奖励，发放组钻石
    if diamond_amount > 0 {
        // 生成幂等键
        let idempotency_key = format!("wish_quality_reward_{}", wish_id);

        // 更新组钻石 (简化，实际上应该用economy_service)
        sqlx::query(
            "UPDATE association_groups SET diamond = diamond + $1, updated_at = NOW() WHERE group_id = $2"
        )
        .bind(diamond_amount)
        .bind(group_id)
        .execute(db)
        .await?;

        // 写钻石流水
        sqlx::query(
            r#"
            INSERT INTO diamond_transactions (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, trace_id, created_at)
            SELECT $1, 'EARN'::diamond_tx_type_enum, $2, diamond - $2, diamond, 'WISH_QUALITY_REWARD', $3, $4, $5, NOW()FROM association_groups WHERE group_id = $1
            "#
        )
        .bind(group_id)
        .bind(diamond_amount)
        .bind(wish_id)
        .bind(&idempotency_key)
        .bind("")
        .execute(db)
        .await?;
    }

    Ok(ApiResponse::success(serde_json::json!({
        "wishId": wish_id,
        "qualityLevel": input.quality_level,
        "diamondReward": diamond_amount,
        "status": "ok"
    })))
}

/// 订单积分/经验奖励审核
/// POST /api/admin/orders/{order_id}/reward-review
///
/// 管理员审核风险订单，决定是否发放爱心积分和组经验
/// 仅用于 point_grant_status=PENDING_REVIEW 或 exp_grant_status=PENDING_REVIEW 的订单
#[utoipa::path(
    post,
    path = "/api/admin/orders/{order_id}/reward-review",
    tag = "后台管理",
    params(
        ("order_id" = i64, description = "订单ID")
    ),
    request_body = OrderRewardReviewInput,
    responses(
        (status = 200, description = "审核成功", body = OrderRewardReviewResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn order_reward_review(
    state: State<Arc<AppState>>,
    _: AdminToken,
    path: Path<i64>,
    body: Json<OrderRewardReviewInput>,
) -> Result<impl web::Responder, CustomError> {
    let order_id = path.into_inner();
    let input = body.into_inner();

    let db = &state.db_pool;

    // 检查订单是否存在
    let order: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT point_grant_status::text, exp_grant_status::text, group_id FROM orders WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_optional(db)
    .await?;

    let (point_status, exp_status, _group_id) = match order {
        Some((p, e, g)) => (p, e, g),
        None => return Err(CustomError::NotFound("订单不存在".into())),
    };

    // 更新积分发放状态
    if point_status == "PENDING_REVIEW" {
        let new_point_status = if input.approve_point {
            "GRANTED"
        } else {
            "REJECTED"
        };
        sqlx::query(
            "UPDATE orders SET point_grant_status = $1::point_grant_status_enum WHERE order_id = $2"
        )
        .bind(new_point_status)
        .bind(order_id)
        .execute(db)
        .await?;
    }

    // 更新经验发放状态
    if exp_status == "PENDING_REVIEW" {
        let new_exp_status = if input.approve_exp {
            "GRANTED"
        } else {
            "REJECTED"
        };
        sqlx::query(
            "UPDATE orders SET exp_grant_status = $1::exp_grant_status_enum WHERE order_id = $2",
        )
        .bind(new_exp_status)
        .bind(order_id)
        .execute(db)
        .await?;
    }

    Ok(ApiResponse::success(serde_json::json!({
        "orderId": order_id,
        "pointGrantStatusUpdated": point_status == "PENDING_REVIEW",
        "expGrantStatusUpdated": exp_status == "PENDING_REVIEW",
        "status": "ok"
    })))
}

/// 获取待审核订单列表
/// GET /api/admin/orders/pending-review
#[utoipa::path(
    get,
    path = "/api/admin/orders/pending-review",
    tag = "后台管理",
    params(
        ("cursor" = Option<String>, Query, description = "游标分页"),
        ("limit" = Option<i32>, Query, description = "每页数量"),
        ("risk_status" = Option<String>, Query, description = "风险状态：SUSPECT/BLOCKED")
    ),
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_pending_review_orders(
    state: State<Arc<AppState>>,
    _: AdminToken,
    query: Query<PendingReviewQuery>,
) -> Result<impl Responder, CustomError> {

    let db = &state.db_pool;
    let limit = query.limit.unwrap_or(20);

    let rows = sqlx::query(
        r#"SELECT order_id, group_id, type, user_id, status, risk_status, risk_detail,
           points_reward, group_exp_reward, point_grant_status, exp_grant_status, created_at
           FROM orders
           WHERE point_grant_status = 'PENDING_REVIEW'::point_grant_status_enum OR exp_grant_status = 'PENDING_REVIEW'
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(db)
    .await?;

    let orders: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "orderId": r.get::<i64, _>("order_id"),
                "groupId": r.get::<i64, _>("group_id"),
                "type": r.get::<String, _>("type"),
                "userId": r.get::<i64, _>("user_id"),
                "status": r.get::<String, _>("status"),
                "riskStatus": r.get::<String, _>("risk_status"),
                "riskDetail": r.get::<Option<serde_json::Value>, _>("risk_detail"),
                "pointsReward": r.get::<i32, _>("points_reward"),
                "groupExpReward": r.get::<i32, _>("group_exp_reward"),
                "pointGrantStatus": r.get::<String, _>("point_grant_status"),
                "expGrantStatus": r.get::<String, _>("exp_grant_status"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339()
            })
        })
        .collect();

    Ok(ApiResponse::success(serde_json::json!({
        "code": 0,
        "message": "success",
        "data": { "orders": orders }
    })))
}

/// 审核风险订单
/// POST /api/admin/orders/{order_id}/review
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOrderInput {
    pub action: String,         // APPROVE=批准发放，REJECT=拒绝发放
    pub point_grant_status: Option<String>, // APPROVE时可设置：GRANTED/REJECTED
    pub exp_grant_status: Option<String>,
    pub remark: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/admin/orders/{order_id}/review",
    tag = "后台管理",
    params(
        ("order_id" = i64, Path, description = "订单ID")
    ),
    request_body = ReviewOrderInput,
    responses(
        (status = 200, description = "审核成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "订单不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn review_order(
    state: State<Arc<AppState>>,
    _: AdminToken,
    path: Path<i64>,
    body: Json<ReviewOrderInput>,
) -> Result<impl Responder, CustomError> {

    let order_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    // 检查订单是否存在
    let order: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT point_grant_status::text, exp_grant_status::text, group_id FROM orders WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_optional(db)
    .await?;

    let (point_status, exp_status, _group_id) = match order {
        Some((p, e, g)) => (p, e, g),
        None => return Err(CustomError::NotFound("订单不存在".into())),
    };

    if input.action == "APPROVE" {
        if point_status == "PENDING_REVIEW" {
            let new_status = input.point_grant_status.as_deref().unwrap_or("GRANTED");
            sqlx::query("UPDATE orders SET point_grant_status = $1::point_grant_status_enum WHERE order_id = $2")
                .bind(new_status)
                .bind(order_id)
                .execute(db)
                .await?;
        }
        if exp_status == "PENDING_REVIEW" {
            let new_status = input.exp_grant_status.as_deref().unwrap_or("GRANTED");
            sqlx::query("UPDATE orders SET exp_grant_status = $1::exp_grant_status_enum WHERE order_id = $2")
                .bind(new_status)
                .bind(order_id)
                .execute(db)
                .await?;
        }
    } else if input.action == "REJECT" {
        if point_status == "PENDING_REVIEW" {
            sqlx::query("UPDATE orders SET point_grant_status = 'REJECTED'::point_grant_status_enum WHERE order_id = $1")
                .bind(order_id)
                .execute(db)
                .await?;
        }
        if exp_status == "PENDING_REVIEW" {
            sqlx::query("UPDATE orders SET exp_grant_status = 'REJECTED'::exp_grant_status_enum WHERE order_id = $1")
                .bind(order_id)
                .execute(db)
                .await?;
        }
    }

    Ok(ApiResponse::success(serde_json::json!({
        "code": 0,
        "message": "success",
        "data": { "orderId": order_id }
    })))
}

/// 获取审计日志
/// GET /api/admin/audit-logs
#[derive(Debug, Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogQuery {
    pub cursor: Option<String>,
    pub limit: Option<i32>,
    pub operator_id: Option<i64>,
    pub action_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/admin/audit-logs",
    tag = "后台管理",
    params(AuditLogQuery),
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_audit_logs(
    state: State<Arc<AppState>>,
    _: AdminToken,
    query: Query<AuditLogQuery>,
) -> Result<impl Responder, CustomError> {

    let db = &state.db_pool;
    let limit = query.limit.unwrap_or(50);

    // 简化的审计日志查询
    let rows = sqlx::query(
        r#"SELECT id, operator_id, action_type, target_type, target_id, detail, ip, created_at
           FROM audit_logs
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(db)
    .await?;

    let logs: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "operatorId": r.get::<i64, _>("operator_id"),
                "actionType": r.get::<String, _>("action_type"),
                "targetType": r.get::<Option<String>, _>("target_type"),
                "targetId": r.get::<Option<i64>, _>("target_id"),
                "detail": r.get::<Option<serde_json::Value>, _>("detail"),
                "ip": r.get::<Option<String>, _>("ip"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339()
            })
        })
        .collect();

    Ok(ApiResponse::success(serde_json::json!({
        "code": 0,
        "message": "success",
        "data": { "logs": logs }
    })))
}

/// 更新组级配置
/// PATCH /api/admin/groups/{group_id}/configs
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupConfigsInput {
    pub normal_order_love_point: Option<i32>,
    pub guest_order_love_point: Option<i32>,
    pub normal_order_group_exp: Option<i32>,
    pub guest_order_group_exp: Option<i32>,
    pub daily_love_point_limit: Option<i32>,
    pub daily_group_exp_limit: Option<i32>,
    pub food_capacity: Option<i32>,
    pub footprint_capacity: Option<i32>,
    pub order_timeout_hours: Option<i32>,
}

#[utoipa::path(
    patch,
    path = "/api/admin/groups/{group_id}/configs",
    tag = "后台管理",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = UpdateGroupConfigsInput,
    responses(
        (status = 200, description = "更新成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_group_configs(
    state: State<Arc<AppState>>,
    _: AdminToken,
    path: Path<i64>,
    body: Json<UpdateGroupConfigsInput>,
) -> Result<impl Responder, CustomError> {

    let group_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    // 检查组是否存在
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT group_id FROM association_groups WHERE group_id = $1"
    )
    .bind(group_id)
    .fetch_optional(db)
    .await?;

    if exists.is_none() {
        return Err(CustomError::NotFound("组不存在".into()));
    }

    // 更新 settings JSONB
    let mut settings = serde_json::json!({});
    if let Some(v) = input.normal_order_love_point {
        settings["normal_order_love_point"] = serde_json::json!(v);
    }
    if let Some(v) = input.guest_order_love_point {
        settings["guest_order_love_point"] = serde_json::json!(v);
    }
    if let Some(v) = input.normal_order_group_exp {
        settings["normal_order_group_exp"] = serde_json::json!(v);
    }
    if let Some(v) = input.guest_order_group_exp {
        settings["guest_order_group_exp"] = serde_json::json!(v);
    }
    if let Some(v) = input.daily_love_point_limit {
        settings["daily_love_point_limit"] = serde_json::json!(v);
    }
    if let Some(v) = input.daily_group_exp_limit {
        settings["daily_group_exp_limit"] = serde_json::json!(v);
    }
    if let Some(v) = input.food_capacity {
        settings["food_capacity"] = serde_json::json!(v);
    }
    if let Some(v) = input.footprint_capacity {
        settings["footprint_capacity"] = serde_json::json!(v);
    }
    if let Some(v) = input.order_timeout_hours {
        settings["order_timeout_hours"] = serde_json::json!(v);
    }

    sqlx::query("UPDATE association_groups SET settings = $1, updated_at = NOW() WHERE group_id = $2")
        .bind(&settings)
        .bind(group_id)
        .execute(db)
        .await?;

    Ok(ApiResponse::success(serde_json::json!({
        "code": 0,
        "message": "success",
        "data": { "groupId": group_id, "updatedConfigs": settings }
    })))
}

/// 管理员补偿积分
/// POST /api/admin/groups/{group_id}/points/compensate
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompensatePointsInput {
    pub user_id: i64,
    pub type_: String, // ADD=增加，REDUCE=扣减
    pub amount: i64,
    pub biz_type: String, // SYSTEM_COMPENSATION / ADMIN_GIFT
    pub remark: String,
}

#[utoipa::path(
    post,
    path = "/api/admin/groups/{group_id}/points/compensate",
    tag = "后台管理",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = CompensatePointsInput,
    responses(
        (status = 200, description = "补偿成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "用户不在该组")
    ),
    security(("bearer_auth" = []))
)]
pub async fn compensate_points(
    state: State<Arc<AppState>>,
    _: AdminToken,
    path: Path<i64>,
    body: Json<CompensatePointsInput>,
) -> Result<impl Responder, CustomError> {

    let group_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    let idempotency_key = format!("compensate_points_{}_{}_{}", group_id, input.user_id, input.amount);

    // 补偿流水
    if input.type_ == "ADD" {
        sqlx::query(
            r#"INSERT INTO love_point_transactions
               (user_id, group_id, type, amount, available_before, available_after, biz_type, biz_id, idempotency_key, created_at)
               SELECT $1, $2, 'ADJUST'::love_point_tx_type_enum, $3, love_point, love_point + $3, $4, $5, $6, NOW()FROM user_group_points WHERE user_id = $1 AND group_id = $2"#,
        )
        .bind(input.user_id)
        .bind(group_id)
        .bind(input.amount)
        .bind(input.biz_type)
        .bind(group_id)
        .bind(&idempotency_key)
        .execute(db)
        .await?;

        sqlx::query(
            "UPDATE user_group_points SET love_point = love_point + $1 WHERE user_id = $2 AND group_id = $3"
        )
        .bind(input.amount)
        .bind(input.user_id)
        .bind(group_id)
        .execute(db)
        .await?;
    } else {
        sqlx::query(
            r#"INSERT INTO love_point_transactions
               (user_id, group_id, type, amount, available_before, available_after, biz_type, biz_id, idempotency_key, created_at)
               SELECT $1, $2, 'ADJUST'::love_point_tx_type_enum, $3, love_point, love_point - $3, $4, $5, $6, NOW()FROM user_group_points WHERE user_id = $1 AND group_id = $2"#,
        )
        .bind(input.user_id)
        .bind(group_id)
        .bind(input.amount)
        .bind(input.biz_type)
        .bind(group_id)
        .bind(&idempotency_key)
        .execute(db)
        .await?;

        sqlx::query(
            "UPDATE user_group_points SET love_point = love_point - $1 WHERE user_id = $2 AND group_id = $3"
        )
        .bind(input.amount)
        .bind(input.user_id)
        .bind(group_id)
        .execute(db)
        .await?;
    }

    Ok(ApiResponse::success(serde_json::json!({
        "code": 0,
        "message": "success",
        "data": { "userId": input.user_id, "groupId": group_id, "amount": input.amount }
    })))
}

/// 管理员补偿钻石
/// POST /api/admin/groups/{group_id}/diamonds/compensate
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompensateDiamondsInput {
    pub type_: String, // ADD=增加，REDUCE=扣减
    pub amount: i64,
    pub remark: String,
}

#[utoipa::path(
    post,
    path = "/api/admin/groups/{group_id}/diamonds/compensate",
    tag = "后台管理",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = CompensateDiamondsInput,
    responses(
        (status = 200, description = "补偿成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn compensate_diamonds(
    state: State<Arc<AppState>>,
    _: AdminToken,
    path: Path<i64>,
    body: Json<CompensateDiamondsInput>,
) -> Result<impl Responder, CustomError> {

    let group_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    let idempotency_key = format!("compensate_diamonds_{}_{}_{}", group_id, input.type_, input.amount);

    if input.type_ == "ADD" {
        sqlx::query(
            r#"INSERT INTO diamond_transactions
               (group_id, type, amount, balance_before, balance_after, biz_type, idempotency_key, created_at)
               SELECT $1, 'ADJUST'::diamond_tx_type_enum, $2, diamond, diamond + $2, 'ADMIN_COMPENSATION', $3, NOW()FROM association_groups WHERE group_id = $1"#,
        )
        .bind(group_id)
        .bind(input.amount)
        .bind(&idempotency_key)
        .execute(db)
        .await?;

        sqlx::query("UPDATE association_groups SET diamond = diamond + $1 WHERE group_id = $2")
            .bind(input.amount)
            .bind(group_id)
            .execute(db)
            .await?;
    } else {
        sqlx::query(
            r#"INSERT INTO diamond_transactions
               (group_id, type, amount, balance_before, balance_after, biz_type, idempotency_key, created_at)
               SELECT $1, 'ADJUST'::diamond_tx_type_enum, $2, diamond, diamond - $2, 'ADMIN_COMPENSATION', $3, NOW()FROM association_groups WHERE group_id = $1"#,
        )
        .bind(group_id)
        .bind(input.amount)
        .bind(&idempotency_key)
        .execute(db)
        .await?;

        sqlx::query("UPDATE association_groups SET diamond = diamond - $1 WHERE group_id = $2")
            .bind(input.amount)
            .bind(group_id)
            .execute(db)
            .await?;
    }

    Ok(ApiResponse::success(serde_json::json!({
        "code": 0,
        "message": "success",
        "data": { "groupId": group_id, "amount": input.amount }
    })))
}

// ============== FSD v2 请求结构体 ==============

/// 待审核订单查询参数
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingReviewQuery {
    pub cursor: Option<String>,
    pub limit: Option<i32>,
    pub risk_status: Option<String>,
}

/// 心愿质量奖励输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishQualityRewardInput {
    pub quality_level: String, // NONE/NORMAL/GOOD/EXCELLENT
    pub remark: Option<String>,
}

/// 订单奖励审核输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderRewardReviewInput {
    pub approve_point: bool, // 是否批准积分发放
    pub approve_exp: bool,   // 是否批准经验发放
    pub remark: Option<String>,
}

// ============================================================
// §24.7 菜品审核（food_audit_logs）
// ============================================================

/// 待审核菜品列表查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PendingFoodAuditQuery {
    pub cursor: Option<String>,         // 上一页响应里的 next_cursor
    pub limit: Option<i64>,
}

/// Cursor payload: 编码 (created_at, food_id) 二元组
/// 用于待审核菜品列表排序 (created_at ASC) 的稳定分页
#[derive(Debug, Serialize, Deserialize)]
struct PendingFoodCursor {
    created_at: chrono::DateTime<chrono::Utc>,
    food_id: i64,
}

/// 审核结果
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodAuditOut {
    pub audit_id: i64,
    pub food_id: i64,
    pub food_name: String,
    pub from_status: String,
    pub to_status: String,
    pub acted_by: i64,
    pub remark: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 菜品审核结果输出
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingFoodOut {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub group_id: Option<i64>,
    pub created_by: i64,
    pub apply_status: String,
    pub apply_remark: Option<String>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

/// 待审核菜品列表响应（使用项目标准的 CursorPage）
/// 与其他接口保持一致: cursor + next_cursor + has_more + total
pub type PendingFoodAuditListResponse = CursorPage<PendingFoodOut>;

/// 菜品审核结果响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodAuditResult {
    pub food_id: i64,
    pub action: String,            // "APPROVE" / "REJECT"
    pub apply_status: String,      // "APPROVED" / "REJECTED"
    pub food_status: String,       // "NORMAL" / "REJECTED"
    pub audited_by: i64,
}

/// 审核动作输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodAuditInput {
    pub action: String,            // "APPROVE" / "REJECT"
    pub remark: Option<String>,
}

/// 获取待审核菜品列表
#[utoipa::path(
    get,
    path = "/api/admin/foods/pending",
    tag = "菜品审核 (§24.7)",
    params(
        ("cursor" = Option<String>, Query, description = "上一页响应里的 next_cursor"),
        ("limit" = Option<i64>, Query, description = "默认 20")
    ),
    responses(
        (status = 200, description = "获取成功", body = PendingFoodAuditListResponse)
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_pending_food_audits(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
    query: Query<PendingFoodAuditQuery>,
) -> Result<HttpResponse, CustomError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let cursor = query
        .cursor
        .as_deref()
        .and_then(decode_cursor::<PendingFoodCursor>);

    let (c_created_at, c_food_id): (Option<chrono::DateTime<chrono::Utc>>, Option<i64>) =
        match &cursor {
            Some(c) => (Some(c.created_at), Some(c.food_id)),
            None => (None, None),
        };

    let rows = sqlx::query(
        r#"SELECT food_id, food_name, food_photo, group_id, created_by, apply_status, apply_remark, created_at
           FROM foods
           WHERE apply_status = 'PENDING'::apply_status_enum AND is_del = 0
             AND (
               $1::TIMESTAMPTZ IS NULL
               OR created_at > $1
               OR (created_at = $1 AND food_id > $2)
             )
           ORDER BY created_at ASC, food_id ASC
           LIMIT $3"#,
    )
    .bind(c_created_at)
    .bind(c_food_id)
    .bind(limit + 1)
    .fetch_all(&state.db_pool)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM foods WHERE apply_status = 'PENDING'::apply_status_enum AND is_del = 0"
    )
    .fetch_one(&state.db_pool)
    .await?;

    let mut items: Vec<PendingFoodOut> = rows
        .iter()
        .map(|r| PendingFoodOut {
            food_id: r.get("food_id"),
            food_name: r.get("food_name"),
            food_photo: r.get("food_photo"),
            group_id: r.get("group_id"),
            created_by: r.get("created_by"),
            apply_status: r.get("apply_status"),
            apply_remark: r.get("apply_remark"),
            submitted_at: r.get("created_at"),
        })
        .collect();

    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        items.last().map(|last| {
            encode_cursor(&PendingFoodCursor {
                created_at: last.submitted_at,
                food_id: last.food_id,
            })
        })
    } else {
        None
    };

    Ok(ApiResponse::success(PendingFoodAuditListResponse {
        items,
        next_cursor,
        has_more,
        total: Some(total),
    }))
}

/// 审核菜品（通过/拒绝）
#[utoipa::path(
    post,
    path = "/api/admin/foods/{food_id}/audit",
    tag = "菜品审核 (§24.7)",
    params(("food_id" = i64, Path, description = "菜品 ID")),
    request_body = FoodAuditInput,
    responses(
        (status = 200, description = "审核成功", body = FoodAuditResult),
        (status = 400, description = "菜品不在待审核状态"),
        (status = 404, description = "菜品不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn audit_food(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
    body: Json<FoodAuditInput>,
) -> Result<HttpResponse, CustomError> {
    let food_id = path.into_inner();
    let input = body.into_inner();

    if !["APPROVE", "REJECT"].contains(&input.action.as_str()) {
        return Err(CustomError::invalid_parameter("action 必须是 APPROVE 或 REJECT"));
    }

    // apply_status_enum: PENDING / APPROVED / REJECTED
    // food_status_enum:   NORMAL  / OFF      / AUDITING / REJECTED
    // 审核通过:apply_status=APPROVED + food_status=NORMAL;审核拒绝:apply_status=REJECTED + food_status=REJECTED
    let (new_apply_status, new_food_status) = match input.action.as_str() {
        "APPROVE" => ("APPROVED", "NORMAL"),
        "REJECT" => ("REJECTED", "REJECTED"),
        _ => unreachable!(),
    };

    let mut tx = state.db_pool.begin().await?;

    // 校验菜品存在且当前为 PENDING
    let current_status: String = sqlx::query_scalar(
        "SELECT apply_status FROM foods WHERE food_id = $1 AND is_del = 0 FOR UPDATE"
    )
    .bind(food_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| CustomError::food_not_found("菜品不存在"))?;

    if current_status != "PENDING" {
        return Err(CustomError::wish_status_invalid("该菜品不在待审核状态"));
    }

    // 写审核日志
    sqlx::query(
        r#"INSERT INTO food_audit_logs (food_id, action, from_status, to_status, acted_by, remark)
           VALUES ($1, $2, $3::apply_status_enum, $4::apply_status_enum, $5, $6)"#,
    )
    .bind(food_id)
    .bind(if input.action == "APPROVE" { 2 } else { 3 })
    .bind(&current_status)
    .bind(new_food_status)
    .bind(admin.user_id)
    .bind(&input.remark)
    .execute(&mut *tx)
    .await?;

    // 更新菜品:apply_status + food_status
    sqlx::query(
        "UPDATE foods SET apply_status = $1::apply_status_enum, food_status = $2::food_status_enum, approved_at = NOW(), approved_by = $3 WHERE food_id = $4"
    )
    .bind(new_apply_status)
    .bind(new_food_status)
    .bind(admin.user_id)
    .bind(food_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 写审计日志
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, target_id, detail)
           VALUES ($1, 'ADMIN', 'FOOD_AUDIT', 'FOOD', $2, $3)"#,
    )
    .bind(admin.user_id)
    .bind(food_id)
    .bind(serde_json::json!({ "action": input.action, "remark": input.remark }))
    .execute(&state.db_pool)
    .await;

    Ok(ApiResponse::success(FoodAuditResult {
        food_id,
        action: input.action,
        apply_status: new_apply_status.to_string(),
        food_status: new_food_status.to_string(),
        audited_by: admin.user_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- signInRewards7Days array validation ----

    #[test]
    fn validate_sign_in_rewards_7_elements_passes() {
        let v = json!([5, 6, 7, 8, 9, 10, 20]);
        assert!(validate_config("signInRewards7Days", &v).is_ok());
    }

    #[test]
    fn validate_sign_in_rewards_6_elements_rejected() {
        let v = json!([5, 6, 7, 8, 9, 10]);
        let err = validate_config("signInRewards7Days", &v).unwrap_err();
        assert!(format!("{}", err).contains("必须正好 7 个元素"));
    }

    #[test]
    fn validate_sign_in_rewards_8_elements_rejected() {
        let v = json!([5, 6, 7, 8, 9, 10, 20, 99]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_empty_rejected() {
        let v = json!([]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_element_zero_rejected() {
        let v = json!([0, 6, 7, 8, 9, 10, 20]);
        let err = validate_config("signInRewards7Days", &v).unwrap_err();
        assert!(format!("{}", err).contains("元素必须在 1-100"));
    }

    #[test]
    fn validate_sign_in_rewards_element_101_rejected() {
        let v = json!([5, 6, 7, 8, 9, 10, 101]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_element_string_rejected() {
        let v = json!([5, 6, "7", 8, 9, 10, 20]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_not_array_rejected() {
        let v = json!(5);
        let err = validate_config("signInRewards7Days", &v).unwrap_err();
        assert!(format!("{}", err).contains("必须是数组"));
    }

    // ---- integer config range ----

    #[test]
    fn validate_int_config_in_range_passes() {
        let v = json!(50);
        assert!(validate_config("orderPointPercent", &v).is_ok());
    }

    #[test]
    fn validate_int_config_below_min_rejected() {
        let v = json!(0);
        assert!(validate_config("orderPointPercent", &v).is_err());
    }

    #[test]
    fn validate_int_config_above_max_rejected() {
        let v = json!(300);
        assert!(validate_config("orderPointPercent", &v).is_err());
    }

    #[test]
    fn validate_int_config_string_rejected() {
        let v = json!("100");
        let err = validate_config("orderPointPercent", &v).unwrap_err();
        assert!(format!("{}", err).contains("value 必须是整数"));
    }

    #[test]
    fn validate_full_team_bonus_amt_zero_passes() {
        let v = json!(0);
        assert!(validate_config("fullTeamBonusAmt", &v).is_ok());
    }

    #[test]
    fn validate_full_team_bonus_amt_100_passes() {
        let v = json!(100);
        assert!(validate_config("fullTeamBonusAmt", &v).is_ok());
    }

    #[test]
    fn validate_full_team_bonus_amt_101_rejected() {
        let v = json!(101);
        assert!(validate_config("fullTeamBonusAmt", &v).is_err());
    }

    // ---- unknown key ----

    #[test]
    fn validate_unknown_key_rejected() {
        let v = json!(10);
        let err = validate_config("fooBar", &v).unwrap_err();
        assert!(format!("{}", err).contains("未知配置键"));
    }
}
