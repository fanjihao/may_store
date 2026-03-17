use ntex::web::{types::State, HttpResponse, Responder};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    users::models::sign::{SignInResponse, SignInfoResponse},
    users::models::user::{DailyCheckinOut, UserToken},
    users::service::SignService,
};

#[utoipa::path(
    post,
    path = "/users/checkin",
    tag = "签到",
    summary = "每日签到获取爱心积分",
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
