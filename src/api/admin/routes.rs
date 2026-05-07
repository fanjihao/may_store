// API 层 - 后台管理路由
// 处理运营配置、数据统计、权限管理等后台管理功能

use ntex::web::{self, types::State, HttpResponse, Responder, ServiceConfig};
use sqlx::Row;
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

    // TODO: 实际实现应验证配置值并保存到数据库
    println!("Admin config update request: {:?}", body);

    Ok(HttpResponse::Ok().json(&serde_json::json!({"status": "ok"})))
}