// API 层 - 用户组路由
// 处理用户组管理相关的 HTTP 请求

use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    middlewares::auth::UserToken,
    domain::user::{
        InvitationListOut, NewInvitationInput, ConfirmInvitationInput, InvitationRequestOut,
        UnbindRequestInput, GroupInfoOut, BindUserDirectlyInput, GroupUpdateInput,
        GroupPointConfig, GroupPointConfigUpdateInput,
    },
    application::user_service::GroupService,
};

/// 配置用户组路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/invitation")
            .route("", web::get().to(get_invitation))
            .route("", web::post().to(new_invitation))
            .route("/{id}", web::put().to(confirm_invitation))
            .route("/{id}", web::delete().to(cancel_invitation))
            .route("/unbind", web::post().to(unbind_request))
            .route("/group/{id}", web::get().to(get_group_info))
            .route("/bind", web::post().to(bind_user_directly))
            .route("/groups/{group_id}", web::put().to(update_group))
            .route(
                "/groups/{group_id}/point-config",
                web::get().to(get_group_point_config),
            )
            .route(
                "/groups/{group_id}/point-config",
                web::put().to(update_group_point_config),
            ),
    );
}

#[utoipa::path(
    get,
    path = "/invitation",
    tag = "团队",
    summary = "获取当前用户的邀请列表（incoming/outgoing）",
    responses(
        (status = 200, body = InvitationListOut),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_invitation(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = GroupService::get_invitation(token.user_id, &state).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/invitation",
    tag = "团队",
    summary = "发起绑定邀请",
    request_body = NewInvitationInput,
    responses(
        (status = 201, description = "邀请成功，无响应体"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn new_invitation(
    token: UserToken,
    data: Json<NewInvitationInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    GroupService::new_invitation(token.user_id, data.into_inner(), &state).await?;
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    put,
    path = "/invitation/{id}",
    tag = "团队",
    summary = "确认或拒绝邀请 (accept=true 同意)",
    params(("id" = i64, Path, description = "邀请ID")),
    request_body = ConfirmInvitationInput,
    responses(
        (status = 200, description = "操作成功，无响应体"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn confirm_invitation(
    token: UserToken,
    id: Path<(i64,)>,
    data: Json<ConfirmInvitationInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    GroupService::confirm_invitation(token.user_id, id.0, data.into_inner(), &state).await?;
    let _ = state
        .redis_cache
        .delete_user(&token.user_id.to_string())
        .await;
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    delete,
    path = "/invitation/{id}",
    tag = "团队",
    summary = "取消自己发起的待处理邀请",
    params(("id" = i64, Path, description = "邀请ID")),
    responses(
        (status = 200, body = InvitationRequestOut),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn cancel_invitation(
    token: UserToken,
    id: Path<(i64,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    GroupService::cancel_invitation(id.0, &state).await?;
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    post,
    path = "/invitation/unbind",
    tag = "团队",
    summary = "申请解绑（需对方同意）",
    request_body = UnbindRequestInput,
    responses(
        (status = 200, description = "解绑申请已发起"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn unbind_request(
    token: UserToken,
    data: Json<UnbindRequestInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    GroupService::unbind_request(token.user_id, data.into_inner(), &state).await?;
    let _ = state
        .redis_cache
        .delete_user(&token.user_id.to_string())
        .await;
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    get,
    path = "/invitation/group/{id}",
    tag = "团队",
    summary = "获取群组详情及成员列表",
    params(("id" = i64, Path, description = "群组ID")),
    responses(
        (status = 200, body = GroupInfoOut),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_group_info(
    _token: UserToken,
    id: Path<(i64,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = GroupService::get_group_info(id.0, &state).await?;
    Ok(HttpResponse::Ok().json(&res))
}

#[utoipa::path(
    post,
    path = "/invitation/bind",
    tag = "团队",
    summary = "直接绑定用户（无需邀请确认）",
    request_body = BindUserDirectlyInput,
    responses(
        (status = 200, description = "绑定成功"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn bind_user_directly(
    token: UserToken,
    data: Json<BindUserDirectlyInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    GroupService::bind_user_directly(token.user_id, data.into_inner(), &state).await?;
    let _ = state
        .redis_cache
        .delete_user(&token.user_id.to_string())
        .await;
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    put,
    path = "/invitation/groups/{group_id}",
    tag = "团队",
    summary = "修改关联组名称",
    params(("group_id" = i64, Path, description = "关联组ID")),
    request_body = GroupUpdateInput,
    responses(
        (status = 200, description = "修改成功"),
        (status = 400, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_group(
    token: UserToken,
    state: State<Arc<AppState>>,
    path: Path<i64>,
    body: Json<GroupUpdateInput>,
) -> Result<impl Responder, CustomError> {
    GroupService::update_group(token.user_id, path.into_inner(), body.into_inner(), &state).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({ "status": "ok" })))
}

#[utoipa::path(
    get,
    path = "/invitation/groups/{group_id}/point-config",
    tag = "团队",
    summary = "获取关联组积分奖惩配置",
    params(("group_id" = i64, Path, description = "关联组ID")),
    responses(
        (status = 200, body = GroupPointConfig),
        (status = 400, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_group_point_config(
    token: UserToken,
    state: State<Arc<AppState>>,
    path: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let cfg =
        GroupService::get_group_point_config(token.user_id, path.into_inner(), &state).await?;
    Ok(HttpResponse::Ok().json(&cfg))
}

#[utoipa::path(
    put,
    path = "/invitation/groups/{group_id}/point-config",
    tag = "团队",
    summary = "修改关联组积分奖惩配置",
    params(("group_id" = i64, Path, description = "关联组ID")),
    request_body = GroupPointConfigUpdateInput,
    responses(
        (status = 200, description = "修改成功"),
        (status = 400, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_group_point_config(
    token: UserToken,
    state: State<Arc<AppState>>,
    path: Path<i64>,
    body: Json<GroupPointConfigUpdateInput>,
) -> Result<impl Responder, CustomError> {
    GroupService::update_group_point_config(
        token.user_id,
        path.into_inner(),
        body.into_inner(),
        &state,
    )
    .await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({ "status": "ok" })))
}
