// API - 签到路由
// FSD.latest.md compliant - 签到、连续签到、组钻石奖励

use ntex::web::{self, types::State, HttpResponse, Responder, ServiceConfig};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::sign_in_service::SignService;
use crate::config::AppState;
use crate::domain::sign_in::entities::DailyCheckinOut;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置签到路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/sign-in")
            .route("/daily", web::post().to(daily_sign_in))
            .route("/info", web::get().to(get_sign_info)),
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
    pub diamonds_earned: i32,
    pub consecutive_days: i32,
    pub total_diamonds: i32,
}

// ========== 处理器 ==========

/// 每日签到
/// POST /api/sign-in/daily
#[utoipa::path(
    post,
    path = "/api/sign-in/daily",
    tag = "签到",
    responses(
        (status = 200, description = "签到成功", body = DailyCheckinResponse),
        (status = 400, description = "今日已签到"),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn daily_sign_in(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let app_state: Arc<AppState> = state.as_ref().clone();
    let result = SignService::daily_checkin(token, app_state).await?;
    Ok(HttpResponse::Ok().json(&DailyCheckinResponse {
        diamonds_earned: result.diamonds_earned,
        consecutive_days: result.consecutive_days,
        total_diamonds: result.total_diamonds,
    }))
}

/// 获取签到信息
/// GET /api/sign-in/info
#[utoipa::path(
    get,
    path = "/api/sign-in/info",
    tag = "签到",
    responses(
        (status = 200, description = "获取成功", body = SignInfoResponseWrapper),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_sign_info(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let app_state: Arc<AppState> = state.as_ref().clone();
    let result = SignService::get_sign_info(token.user_id, &app_state).await?;
    Ok(HttpResponse::Ok().json(&SignInfoResponseWrapper {
        today_signed: result.today_signed,
        consecutive_days: result.consecutive_days,
        total_sign_days: result.total_sign_days,
        today_diamonds: result.today_diamonds,
        last_sign_date: result.last_sign_date.map(|d| d.to_string()),
    }))
}