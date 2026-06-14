// API - 签到路由
// FSD.latest.md compliant - 签到、连续签到、组钻石奖励

use ntex::web::{self, types::State, Responder, ServiceConfig};
use serde::Deserialize;
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::sign_in_service::SignService;
use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

/// 配置签到路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups/{group_id}/sign-in")
            .route("", web::post().to(sign_in))
            .route("/status", web::get().to(sign_in_status)),
    )
    .service(
        web::scope("/api/groups/{group_id}/sign-ins")
            .route("", web::get().to(get_sign_ins)),
    );
}

// ========== 响应结构 ==========

/// 签到信息响应
#[derive(Debug, Serialize, ToSchema)]
pub struct SignInfoResponseWrapper {
    pub today_signed: bool,
    pub consecutive_days: i32,
    pub total_sign_days: i32,
    pub today_diamonds: i32,
    pub last_sign_date: Option<String>,
}

/// 每日签到响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DailyCheckinResponse {
    pub diamond_reward: i32,
    pub consecutive_days: i32,
    pub total_diamonds: i32,
    pub full_team_bonus: bool,
}

/// 签到记录项
#[derive(Debug, Serialize, ToSchema)]
pub struct SignRecordItem {
    pub sign_id: i64,
    pub user_id: i64,
    pub nickname: Option<String>,
    pub sign_date: String,
    pub consecutive_days: i32,
    pub diamond_reward: i32,
}

/// 组内双方签到状态响应
#[derive(Debug, Serialize, ToSchema)]
pub struct SignInStatusResponse {
    pub date: String,
    pub members: Vec<MemberSignStatus>,
    pub full_team_today: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberSignStatus {
    pub user_id: i64,
    pub signed: bool,
    pub consecutive_days: i32,
}

// ========== 处理器 ==========

/// 当日签到
/// POST /api/groups/{group_id}/sign-in
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/sign-in",
    tag = "签到",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "签到成功", body = DailyCheckinResponse),
        (status = 400, description = "今日已签到"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn sign_in(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let _group_id = *path;
    let app_state = (*state).clone();
    let result = SignService::daily_checkin(token, &app_state).await?;
    Ok(ApiResponse::success(DailyCheckinResponse {
        diamond_reward: result.diamond_reward,
        consecutive_days: result.consecutive_days,
        total_diamonds: result.total_diamonds,
        full_team_bonus: false,
    }))
}

/// 获取组内双方签到状态
/// GET /api/groups/{group_id}/sign-in/status
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/sign-in/status",
    tag = "签到",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = SignInStatusResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn sign_in_status(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let group_id = *path;
    let db = &state.db_pool;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')"
    )
    .bind(group_id)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 获取今天日期
    let today = chrono::Utc::now().date_naive();

    // 获取组成员及其今天的签到状态
    let members = sqlx::query(
        r#"SELECT agm.user_id, sr.sign_date, sr.consecutive_days
           FROM association_group_members agm
           LEFT JOIN sign_in_records sr ON sr.user_id = agm.user_id AND sr.group_id = agm.group_id AND sr.sign_date = $2
           WHERE agm.group_id = $1 AND agm.member_status = 'ACTIVE'"#
    )
    .bind(group_id)
    .bind(today)
    .fetch_all(db)
    .await?;

    let mut member_statuses = Vec::new();
    let mut all_signed = true;
    for r in members {
        let user_id: i64 = r.get("user_id");
        let sign_date: Option<chrono::NaiveDate> = r.get("sign_date");
        let consecutive_days: Option<i32> = r.get("consecutive_days");
        let signed = sign_date.is_some();
        if !signed {
            all_signed = false;
        }
        member_statuses.push(MemberSignStatus {
            user_id,
            signed,
            consecutive_days: consecutive_days.unwrap_or(0),
        });
    }

    Ok(ApiResponse::success(SignInStatusResponse {
        date: today.to_string(),
        members: member_statuses,
        full_team_today: all_signed,
    }))
}

/// 获取签到记录
/// GET /api/groups/{group_id}/sign-ins
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/sign-ins",
    tag = "签到",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        ("year_month" = Option<String>, Query, description = "年月，如2026-06")
    ),
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_sign_ins(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: ntex::web::types::Path<i64>,
    query: ntex::web::types::Query<SignInsQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = *path;
    let db = &state.db_pool;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')"
    )
    .bind(group_id)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    let date_filter = if let Some(ref ym) = query.year_month {
        format!("AND TO_CHAR(sr.sign_date, 'YYYY-MM') = '{}'", ym)
    } else {
        String::new()
    };

    let sql = format!(
        r#"SELECT sr.id, sr.user_id, sr.sign_date, sr.consecutive_days, sr.diamond_reward, u.nick_name
           FROM sign_in_records sr
           JOIN users u ON u.user_id = sr.user_id
           WHERE sr.group_id = $1 {}
           ORDER BY sr.sign_date DESC
           LIMIT 31"#,
        date_filter
    );

    let records = sqlx::query(&sql).bind(group_id).fetch_all(db).await?;

    let items: Vec<serde_json::Value> = records
        .iter()
        .map(|r| {
            serde_json::json!({
                "signId": r.get::<i64, _>("id"),
                "userId": r.get::<i64, _>("user_id"),
                "nickname": r.get::<Option<String>, _>("nick_name"),
                "signDate": r.get::<chrono::NaiveDate, _>("sign_date").to_string(),
                "consecutiveDays": r.get::<i32, _>("consecutive_days"),
                "diamondsEarned": r.get::<i32, _>("diamond_reward")
            })
        })
        .collect();

    Ok(ApiResponse::success(serde_json::json!({
        "records": items,
        "totalCount": items.len()
    })))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SignInsQuery {
    pub year_month: Option<String>,
}