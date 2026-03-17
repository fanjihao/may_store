use ntex::web::{
    types::{Json, Path, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    users::models::group::{
        BindUserDirectlyInput, ConfirmInvitationInput, GroupInfoOut, GroupPointConfig,
        GroupPointConfigUpdateInput, GroupUpdateInput, InvitationListOut, InvitationRequestOut,
        NewInvitationInput, UnbindRequestInput,
    },
    users::models::user::UserToken,
    users::service::GroupService,
};

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
    _: UserToken,
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
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    put,
    path = "/invitation/groups/{group_id}",
    tag = "团队",
    summary = "修改关联组名称",
    request_body = GroupUpdateInput,
    params(
        ("group_id" = i64, Path, description = "关联组ID")
    ),
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
    params(
        ("group_id" = i64, Path, description = "关联组ID")
    ),
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
    request_body = GroupPointConfigUpdateInput,
    params(
        ("group_id" = i64, Path, description = "关联组ID")
    ),
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
    body: Json<crate::users::models::group::GroupPointConfigUpdateInput>,
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
