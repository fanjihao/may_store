// API - 成就路由
// FSD.latest.md compliant - 成就列表、成就墙

use ntex::web::{self, types::State, HttpResponse, Responder, ServiceConfig};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置成就路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups/{group_id}/achievements")
            .route("", web::get().to(get_achievements))
            .route("/wall", web::get().to(get_achievement_wall)),
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
    pub total: i32,
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
    security(("cookie_auth" = []))
)]
pub async fn get_achievements(
    state: State<Arc<AppState>>,
    token: UserToken,
    group_id: ntex::web::types::Path<i64>,
    query: ntex::web::types::Query<AchievementQuery>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')"
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 构建类别过滤
    let category_filter = if let Some(ref cat) = query.category {
        format!("AND a.category = '{}'", cat)
    } else {
        String::new()
    };

    // 获取成就定义和用户解锁状态
    let sql = format!(
        r#"
        SELECT a.code as achievement_id, a.name, a.description, a.category, a.icon,
               ua.unlocked_at,
               CASE WHEN ua.id IS NOT NULL THEN true ELSE false END as unlocked
        FROM achievement_definitions a
        LEFT JOIN user_achievements ua ON ua.achievement_id = a.id AND ua.user_id = $1
        WHERE a.is_active = true {}
        ORDER BY a.category, a.display_order
        "#,
        category_filter
    );

    let rows = sqlx::query(&sql).bind(token.user_id).fetch_all(db).await?;

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
                progress: None,
                total: None,
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(&AchievementsResponse { achievements }))
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
    security(("cookie_auth" = []))
)]
pub async fn get_achievement_wall(
    state: State<Arc<AppState>>,
    token: UserToken,
    group_id: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')"
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 获取总成就数和已解锁数
    let total_count: i32 =
        sqlx::query_scalar("SELECT COUNT(*) FROM achievement_definitions WHERE is_active = true")
            .fetch_one(db)
            .await?;

    let unlocked_count: i32 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT ua.achievement_id) FROM user_achievements ua JOIN achievement_definitions a ON a.id = ua.achievement_id WHERE ua.user_id = $1 AND a.is_active = true"
    )
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    // 获取已解锁成就
    let unlocked_rows = sqlx::query(
        r#"
        SELECT a.code as achievement_id, a.name, a.category, ua.unlocked_at
        FROM user_achievements ua
        JOIN achievement_definitions a ON a.id = ua.achievement_id
        WHERE ua.user_id = $1 AND a.is_active = true
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
                description: None,
                category: r.get("category"),
                icon: None,
                unlocked: true,
                unlocked_at: unlocked_at.map(|dt| dt.to_rfc3339()),
                progress: None,
                total: None,
            }
        })
        .collect();

    // 查找下一个可解锁成就（简化：取第一个未解锁的 USER 类型）
    let next_unlock = if unlocked_count < total_count {
        let next_rows = sqlx::query(
            r#"
            SELECT a.code as achievement_id, a.name, 0 as progress, a.requirement_value as total
            FROM achievement_definitions a
            LEFT JOIN user_achievements ua ON ua.achievement_id = a.id AND ua.user_id = $1
            WHERE a.is_active = true AND ua.id IS NULL AND a.category = 'USER'
            ORDER BY a.display_order
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
            total: r.get("total"),
        })
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(&AchievementWallResponse {
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
