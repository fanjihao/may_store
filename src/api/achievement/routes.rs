// API - 成就路由
// FSD.latest.md compliant - 成就列表、成就墙

use ntex::web::{self, types::State, Responder, ServiceConfig};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::middlewares::target_group::require_active_target_group_member;
use crate::utils::response::ApiResponse;

/// 配置成就路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/achievements").route(web::get().to(get_achievements)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/achievements/wall")
            .route(web::get().to(get_achievement_wall)),
    );
}

// ========== 响应结构 ==========

/// 成就项响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AchievementItem {
    pub achievement_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub icon: Option<String>,
    pub unlocked: bool,
    pub unlocked_at: Option<String>,
    pub progress: Option<i32>,
    pub total: Option<i32>,
}

/// 成就列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct AchievementsResponse {
    pub achievements: Vec<AchievementItem>,
}

/// 成就墙响应
#[derive(Debug, Serialize, ToSchema)]
pub struct AchievementWallResponse {
    pub total_achievements: i32,
    pub unlocked_count: i32,
    pub achievements: Vec<AchievementItem>,
    pub next_unlock: Option<NextUnlockItem>,
}

/// 下一成就解锁信息
#[derive(Debug, Serialize, ToSchema)]
pub struct NextUnlockItem {
    pub achievement_id: String,
    pub name: String,
    pub progress: i32,
    /// v3 仅定义 rule_config JSONB，未定义可通用解码的目标值字段。
    pub total: Option<i32>,
}

// ========== 处理器 ==========

/// 获取成就列表
/// GET /api/groups/{group_id}/achievements?category=USER|GROUP
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/achievements",
    tag = "成就",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        ("category" = Option<String>, Query, description = "成就类别: USER/GROUP")
    ),
    responses(
        (status = 200, description = "获取成功", body = AchievementsResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_achievements(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    group_id: ntex::web::types::Path<i64>,
    query: ntex::web::types::Query<AchievementQuery>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;

    require_active_target_group_member(db, token.user_id, gid).await?;

    // 参数化查询 + 枚举白名单,避免 SQL 注入
    let category_filter: Option<&str> = match query.category.as_deref() {
        Some(c) if matches!(c, "USER" | "GROUP") => Some(c),
        Some(_) => return Err(CustomError::BadRequest("category 非法".into())),
        None => None,
    };

    // 获取成就定义和用户解锁状态。
    // category 是自定义枚举，读取时必须 ::text；筛选 bind 也必须转回 enum。
    let rows = match category_filter {
        Some(c) => sqlx::query(
            r#"
            SELECT a.code as achievement_id, a.name, a.description, a.category::text AS category, a.icon,
                   ua.progress, ua.unlocked_at,
                   (ua.unlocked_at IS NOT NULL) AS unlocked
            FROM achievements a
            LEFT JOIN user_achievements ua
              ON ua.achievement_id = a.achievement_id AND ua.user_id = $1
            WHERE a.is_enabled = true
              AND a.category = $2::achievement_category_enum
            ORDER BY a.category, a.created_at, a.achievement_id
            "#,
        )
        .bind(token.user_id)
        .bind(c)
        .fetch_all(db)
        .await?,
        None => sqlx::query(
            r#"
            SELECT a.code as achievement_id, a.name, a.description, a.category::text AS category, a.icon,
                   ua.progress, ua.unlocked_at,
                   (ua.unlocked_at IS NOT NULL) AS unlocked
            FROM achievements a
            LEFT JOIN user_achievements ua
              ON ua.achievement_id = a.achievement_id AND ua.user_id = $1
            WHERE a.is_enabled = true
            ORDER BY a.category, a.created_at, a.achievement_id
            "#,
        )
        .bind(token.user_id)
        .fetch_all(db)
        .await?,
    };

    let achievements: Vec<AchievementItem> = rows
        .iter()
        .map(|r| {
            let unlocked: bool = r.get("unlocked");
            let unlocked_at: Option<chrono::DateTime<chrono::Utc>> = r.get("unlocked_at");
            AchievementItem {
                achievement_id: r.get("achievement_id"),
                name: r.get("name"),
                description: r.get("description"),
                category: r.get("category"),
                icon: r.get("icon"),
                unlocked,
                unlocked_at: unlocked_at.map(|dt| dt.to_rfc3339()),
                progress: r.get("progress"),
                total: None,
            }
        })
        .collect();

    Ok(ApiResponse::success(AchievementsResponse { achievements }))
}

/// 获取成就墙
/// GET /api/groups/{group_id}/achievements/wall
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/achievements/wall",
    tag = "成就",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = AchievementWallResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_achievement_wall(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    group_id: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;

    require_active_target_group_member(db, token.user_id, gid).await?;

    // 获取总成就数和已解锁数
    let total_count: i32 =
        sqlx::query_scalar("SELECT COUNT(*)::INT FROM achievements WHERE is_enabled = true")
            .fetch_one(db)
            .await?;

    let unlocked_count: i32 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT ua.achievement_id)::INT \
         FROM user_achievements ua \
         JOIN achievements a ON a.achievement_id = ua.achievement_id \
         WHERE ua.user_id = $1 AND ua.unlocked_at IS NOT NULL AND a.is_enabled = true",
    )
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    // 获取已解锁成就
    let unlocked_rows = sqlx::query(
        r#"
        SELECT a.code as achievement_id, a.name, a.description,
               a.category::text AS category, a.icon, ua.progress, ua.unlocked_at
        FROM user_achievements ua
        JOIN achievements a ON a.achievement_id = ua.achievement_id
        WHERE ua.user_id = $1
          AND ua.unlocked_at IS NOT NULL
          AND a.is_enabled = true
        ORDER BY ua.unlocked_at DESC
        LIMIT 20
        "#,
    )
    .bind(token.user_id)
    .fetch_all(db)
    .await?;

    let achievements: Vec<AchievementItem> = unlocked_rows
        .iter()
        .map(|r| {
            let unlocked_at: Option<chrono::DateTime<chrono::Utc>> = r.get("unlocked_at");
            AchievementItem {
                achievement_id: r.get("achievement_id"),
                name: r.get("name"),
                description: r.get("description"),
                category: r.get("category"),
                icon: r.get("icon"),
                unlocked: true,
                unlocked_at: unlocked_at.map(|dt| dt.to_rfc3339()),
                progress: Some(r.get("progress")),
                total: None,
            }
        })
        .collect();

    // 查找下一个可解锁成就（取第一个未解锁的 USER 类型）。
    // progress 来自 user_achievements；没有进度行时明确为 0。
    // v3 未规定 rule_config 的统一阈值键，因此 total 明确返回 null。
    let next_unlock = if unlocked_count < total_count {
        let next_rows = sqlx::query(
            r#"
            SELECT a.code as achievement_id, a.name, COALESCE(ua.progress, 0)::INT AS progress
            FROM achievements a
            LEFT JOIN user_achievements ua
              ON ua.achievement_id = a.achievement_id AND ua.user_id = $1
            WHERE a.is_enabled = true
              AND ua.unlocked_at IS NULL
              AND a.category = 'USER'::achievement_category_enum
            ORDER BY a.created_at, a.achievement_id
            LIMIT 1
            "#,
        )
        .bind(token.user_id)
        .fetch_optional(db)
        .await?;

        next_rows.map(|r| NextUnlockItem {
            achievement_id: r.get("achievement_id"),
            name: r.get("name"),
            progress: r.get("progress"),
            total: None,
        })
    } else {
        None
    };

    Ok(ApiResponse::success(AchievementWallResponse {
        total_achievements: total_count,
        unlocked_count,
        achievements,
        next_unlock,
    }))
}

/// 成就查询参数
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct AchievementQuery {
    pub category: Option<String>,
}
