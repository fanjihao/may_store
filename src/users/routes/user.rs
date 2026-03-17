use ntex::web::{
    types::{Json, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    users::{
        models::user::{
            IsRegisterQuery, IsRegisterResponse, LoginInput, LoginResponse, ProfileUpdateInput,
            RegisterInput, RoleSwitchInput, RoleSwitchResult, UserInfoResponse, UserPublic,
            UserToken,
        },
        service::UserService,
    },
};

#[utoipa::path(
    post,
    path = "/register",
    request_body = RegisterInput,
    tag = "用户",
    responses(
        (status = 201, description = "注册成功，无响应体"),
        (status = 400, body = CustomError)
    )
)]
pub async fn register(
    data: Json<RegisterInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    UserService::register(data.into_inner(), &state).await?;
    Ok(HttpResponse::Created().finish())
}

#[utoipa::path(
    post,
    path = "/login",
    tag = "用户",
    summary = "账号密码登录，返回 Token 与用户信息",
    request_body = LoginInput,
    responses(
        (status = 200, body = LoginResponse),
        (status = 400, body = CustomError)
    )
)]
pub async fn login(
    user: Json<LoginInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = UserService::login(user.into_inner(), &state).await?;
    Ok(Json(res))
}

#[utoipa::path(
    get,
    path = "/users",
    operation_id = "get_current_info",
    tag = "用户",
    summary = "获取当前登录用户信息",
    responses(
        (status = 200, body = UserPublic),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_current_info(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = UserService::get_current_info(token.user_id, &state).await?;
    Ok(Json(res))
}

#[utoipa::path(
    get,
    path = "/getInfoByUsername",
    operation_id = "get_user_info",
    tag = "用户",
    summary = "根据用户名获取用户信息",
    params(IsRegisterQuery),
    responses((status = 200, body = UserInfoResponse), (status = 401, body = CustomError))
)]
pub async fn get_user_info(
    q: Query<IsRegisterQuery>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = UserService::get_user_info(&q.username, &state).await?;
    Ok(Json(res))
}

#[utoipa::path(
    get,
    path = "/users/is-register",
    operation_id = "is_register",
    tag = "用户",
    summary = "判断用户名是否已注册",
    params(IsRegisterQuery),
    responses((status = 200, body = IsRegisterResponse), (status = 400, body = CustomError))
)]
pub async fn is_register(
    q: Query<IsRegisterQuery>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = UserService::is_register(&q.username, &state).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/users",
    operation_id = "wx_change_info",
    tag = "用户",
    request_body = ProfileUpdateInput,
    responses((status = 200, body = UserPublic), (status = 400, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn change_info(
    _: UserToken,
    data: Json<ProfileUpdateInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = UserService::change_info(data.into_inner(), &state).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post, 
    path="/users/role-switch", 
    tag="用户", 
    request_body=RoleSwitchInput, 
    responses((status=200, body=RoleSwitchResult)), 
    security(("cookie_auth"=[]))
)]
pub async fn switch_role(
    token: UserToken,
    state: State<Arc<AppState>>,
    body: Json<RoleSwitchInput>,
) -> Result<impl Responder, CustomError> {
    let res = UserService::switch_role(token.user_id, body.into_inner(), &state).await?;
    Ok(HttpResponse::Ok().json(&res))
}
