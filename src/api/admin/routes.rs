// API 层 - 后台管理路由
// 处理运营配置、数据统计、权限管理等后台管理功能

use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置后台管理路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/admin")
            .route("/stats", web::get().to(get_stats))
            .route("/groups", web::get().to(list_groups))
            .route("/users", web::get().to(list_all_users))
            .route("/config", web::get().to(get_config))
            .route("/config", web::put().to(update_config))
            // FSD v2: 心愿质量奖励审核
            .route("/wishes/{wish_id}/quality-reward", web::post().to(wish_quality_reward))
            // FSD v2: 订单积分/经验审核
            .route("/orders/{order_id}/reward-review", web::post().to(order_reward_review))
    );
}

/// 获取系统统计数据
/// 管理员查看整体运营数据
#[utoipa::path(
    get,
    path = "/admin/stats",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_stats(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    // 检查是否为管理员角色
    let is_admin = token.user.as_ref().map(|u| u.role == crate::domain::user::UserRole::Admin).unwrap_or(false);
    if !is_admin {
        return Err(CustomError::Forbidden("需要管理员权限".into()));
    }

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

    let active_orders: i64 = sqlx::query("SELECT COUNT(*) FROM orders WHERE status NOT IN ('COMPLETED', 'CANCELLED', 'REJECTED')")
        .fetch_one(db)
        .await?
        .get(0);

    let total_diamonds: i64 = sqlx::query("SELECT COALESCE(SUM(diamond), 0) FROM association_groups")
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

    Ok(HttpResponse::Ok().json(&stats))
}

/// 获取所有组列表
#[utoipa::path(
    get,
    path = "/admin/groups",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("cookie_auth" = []))
)]
pub async fn list_groups(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let is_admin = token.user.as_ref().map(|u| u.role == crate::domain::user::UserRole::Admin).unwrap_or(false);
    if !is_admin {
        return Err(CustomError::Forbidden("需要管理员权限".into()));
    }

    let groups = sqlx::query(
        "SELECT group_id, group_name, diamond, member_count, created_at FROM association_groups ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&state.db_pool)
    .await?;

    let result: Vec<serde_json::Value> = groups.into_iter()
        .map(|r| serde_json::json!({
            "groupId": r.get::<i64, _>("group_id"),
            "groupName": r.get::<String, _>("group_name"),
            "diamond": r.get::<i32, _>("diamond"),
            "memberCount": r.get::<i32, _>("member_count"),
            "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        }))
        .collect();

    Ok(HttpResponse::Ok().json(&result))
}

/// 获取所有用户列表
#[utoipa::path(
    get,
    path = "/admin/users",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("cookie_auth" = []))
)]
pub async fn list_all_users(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let is_admin = token.user.as_ref().map(|u| u.role == crate::domain::user::UserRole::Admin).unwrap_or(false);
    if !is_admin {
        return Err(CustomError::Forbidden("需要管理员权限".into()));
    }

    let users = sqlx::query(
        "SELECT user_id, username, nick_name, role, love_point, diamond, created_at FROM users ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&state.db_pool)
    .await?;

    let result: Vec<serde_json::Value> = users.into_iter()
        .map(|r| serde_json::json!({
            "userId": r.get::<i64, _>("user_id"),
            "username": r.get::<String, _>("username"),
            "nickName": r.get::<Option<String>, _>("nick_name"),
            "role": r.get::<String, _>("role"),
            "lovePoint": r.get::<i32, _>("love_point"),
            "diamond": r.get::<i32, _>("diamond"),
            "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        }))
        .collect();

    Ok(HttpResponse::Ok().json(&result))
}

/// 获取系统配置
#[utoipa::path(
    get,
    path = "/admin/config",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_config(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let is_admin = token.user.as_ref().map(|u| u.role == crate::domain::user::UserRole::Admin).unwrap_or(false);
    if !is_admin {
        return Err(CustomError::Forbidden("需要管理员权限".into()));
    }

    // 返回默认配置（简化实现）
    let config = serde_json::json!({
        "signRewardDaily": 5,
        "signRewardConsecutive": 10,
        "orderPointPercent": 100,
        "diamondUnlockCost": 100,
        "defaultFootprintCapacity": 50
    });

    Ok(HttpResponse::Ok().json(&config))
}

/// 更新系统配置
#[utoipa::path(
    put,
    path = "/admin/config",
    tag = "后台管理",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "更新成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限")
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_config(
    state: State<Arc<AppState>>,
    token: UserToken,
    body: web::types::Json<serde_json::Value>,
) -> Result<impl Responder, CustomError> {
    let is_admin = token.user.as_ref().map(|u| u.role == crate::domain::user::UserRole::Admin).unwrap_or(false);
    if !is_admin {
        return Err(CustomError::Forbidden("需要管理员权限".into()));
    }

    let db = &state.db_pool;
    let config = body.into_inner();

    // 验证并更新配置项
    if let Some(sign_reward_daily) = config.get("signRewardDaily").and_then(|v| v.as_i64()) {
        if sign_reward_daily < 1 || sign_reward_daily > 100 {
            return Err(CustomError::BadRequest("每日签到奖励必须在1-100之间".into()));
        }
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ('sign_reward_daily', $1, NOW()) ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()"
        )
        .bind(sign_reward_daily as i32)
        .execute(db)
        .await?;
    }

    if let Some(sign_reward_consecutive) = config.get("signRewardConsecutive").and_then(|v| v.as_i64()) {
        if sign_reward_consecutive < 1 || sign_reward_consecutive > 100 {
            return Err(CustomError::BadRequest("连续签到奖励必须在1-100之间".into()));
        }
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ('sign_reward_consecutive', $1, NOW()) ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()"
        )
        .bind(sign_reward_consecutive as i32)
        .execute(db)
        .await?;
    }

    if let Some(order_point_percent) = config.get("orderPointPercent").and_then(|v| v.as_i64()) {
        if order_point_percent < 1 || order_point_percent > 200 {
            return Err(CustomError::BadRequest("订单积分百分比必须在1-200之间".into()));
        }
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ('order_point_percent', $1, NOW()) ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()"
        )
        .bind(order_point_percent as i32)
        .execute(db)
        .await?;
    }

    if let Some(diamond_unlock_cost) = config.get("diamondUnlockCost").and_then(|v| v.as_i64()) {
        if diamond_unlock_cost < 10 || diamond_unlock_cost > 10000 {
            return Err(CustomError::BadRequest("钻石解锁费用必须在10-10000之间".into()));
        }
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ('diamond_unlock_cost', $1, NOW()) ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()"
        )
        .bind(diamond_unlock_cost as i32)
        .execute(db)
        .await?;
    }

    if let Some(default_capacity) = config.get("defaultFootprintCapacity").and_then(|v| v.as_i64()) {
        if default_capacity < 10 || default_capacity > 1000 {
            return Err(CustomError::BadRequest("默认足迹容量必须在10-1000之间".into()));
        }
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ('default_footprint_capacity', $1, NOW()) ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()"
        )
        .bind(default_capacity as i32)
        .execute(db)
        .await?;
    }

    println!("Admin config updated: {:?}", config);

    Ok(HttpResponse::Ok().json(&serde_json::json!({"status": "ok"})))
}

/// 心愿质量奖励审核
/// POST /api/admin/wishes/{wish_id}/quality-reward
///
/// 管理员查看打卡反馈后，根据质量发放额外组钻石奖励
/// 质量等级: NONE/NORMAL/GOOD/EXCELLENT
/// 必须幂等，同一心愿额外钻石奖励只发放一次
#[utoipa::path(
    post,
    path = "/admin/wishes/{wish_id}/quality-reward",
    tag = "后台管理",
    params(
        ("wish_id" = i64, description = "心愿ID")
    ),
    request_body = WishQualityRewardInput,
    responses(
        (status = 200, description = "奖励成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "心愿不存在"),
        (status = 409, description = "已发放过奖励")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_quality_reward(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    body: Json<WishQualityRewardInput>,
) -> Result<impl Responder, CustomError> {
    let is_admin = token.user.as_ref().map(|u| u.role == crate::domain::user::UserRole::Admin).unwrap_or(false);
    if !is_admin {
        return Err(CustomError::Forbidden("需要管理员权限".into()));
    }

    let wish_id = path.into_inner();
    let input = body.into_inner();

    let db = &state.db_pool;

    // 检查心愿是否存在且处于FINISHED状态
    let wish: Option<(String, i64)> = sqlx::query_as(
        "SELECT status::text, group_id FROM wishes WHERE wish_id = $1"
    )
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
        "SELECT diamond_reward FROM wishes WHERE wish_id = $1 AND diamond_reward > 0"
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
            quality_review_status = 'REVIEWED',
            quality_reviewer_id = $1,
            quality_remark = $2,
            diamond_reward = $3,
            updated_at = NOW()
        WHERE wish_id = $4
        "#
    )
    .bind(token.user_id)
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
            SELECT $1, 'EARN', $2, diamond - $2, diamond, 'WISH_QUALITY_REWARD', $3, $4, $5, NOW()
            FROM association_groups WHERE group_id = $1
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

    Ok(HttpResponse::Ok().json(&serde_json::json!({
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
    path = "/admin/orders/{order_id}/reward-review",
    tag = "后台管理",
    params(
        ("order_id" = i64, description = "订单ID")
    ),
    request_body = OrderRewardReviewInput,
    responses(
        (status = 200, description = "审核成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权限"),
        (status = 404, description = "订单不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn order_reward_review(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    body: Json<OrderRewardReviewInput>,
) -> Result<impl web::Responder, CustomError> {
    let is_admin = token.user.as_ref().map(|u| u.role == crate::domain::user::UserRole::Admin).unwrap_or(false);
    if !is_admin {
        return Err(CustomError::Forbidden("需要管理员权限".into()));
    }

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

    let (point_status, exp_status, group_id) = match order {
        Some((p, e, g)) => (p, e, g),
        None => return Err(CustomError::NotFound("订单不存在".into())),
    };

    // 更新积分发放状态
    if point_status == "PENDING_REVIEW" {
        let new_point_status = if input.approve_point { "GRANTED" } else { "REJECTED" };
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
        let new_exp_status = if input.approve_exp { "GRANTED" } else { "REJECTED" };
        sqlx::query(
            "UPDATE orders SET exp_grant_status = $1::exp_grant_status_enum WHERE order_id = $2"
        )
        .bind(new_exp_status)
        .bind(order_id)
        .execute(db)
        .await?;
    }

    println!("Admin order reward review: order_id={}, point_approved={}, exp_approved={}",
        order_id, input.approve_point, input.approve_exp);

    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "orderId": order_id,
        "pointGrantStatusUpdated": point_status == "PENDING_REVIEW",
        "expGrantStatusUpdated": exp_status == "PENDING_REVIEW",
        "status": "ok"
    })))
}

// ============== FSD v2 请求结构体 ==============

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
    pub approve_exp: bool,    // 是否批准经验发放
    pub remark: Option<String>,
}