// API 层 - 签到路由
// 处理签到相关的 HTTP 请求

use ntex::web::{self, types::State, HttpResponse, Responder, ServiceConfig};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    middlewares::auth::UserToken,
    domain::sign_in::entities::{SignInResponse, SignInfoResponse, DailyCheckinOut},
    application::sign_in_service::SignService,
};

/// 配置签到路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(web::scope("/users").route("/checkin", web::post().to(daily_checkin)))
        .service(
            web::scope("/sign")
                .route("", web::post().to(sign_in))
                .route("/info", web::get().to(get_sign_info)),
        );
}

#[utoipa::path(
    post,
    path = "/users/checkin",
    tag = "签到",
    summary = "每日签到获取钻石",
    responses(
        (status = 201, body = DailyCheckinOut),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn daily_checkin(
    user_token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = SignService::daily_checkin(user_token, &state).await?;
    Ok(HttpResponse::Created().json(&res))
}

#[utoipa::path(
    post,
    path = "/sign",
    tag = "签到",
    responses(
        (status = 200, body = SignInResponse),
        (status = 400, description = "今日已签到"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn sign_in(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = SignService::sign_in(token.user_id, &state).await?;
    Ok(HttpResponse::Ok().json(&res))
}

#[utoipa::path(
    get,
    path = "/sign/info",
    tag = "签到",
    responses(
        (status = 200, body = SignInfoResponse),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_sign_info(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = SignService::get_sign_info(token.user_id, &state).await?;
    Ok(HttpResponse::Ok().json(&res))
}
