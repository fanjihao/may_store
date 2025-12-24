use std::sync::Arc;

use ntex::web::types::{Json, State};
use ntex::web::{HttpResponse, Responder};

use crate::errors::CustomError;
use crate::game_im::rest::ImRestClient;
use crate::models::game_im::{ImCreateRoomIn, ImDismissRoomIn, ImDismissRoomOut, ImJoinRoomOut, ImRoomListOut, ImRoomOut};
use crate::models::users::UserToken;
use crate::AppState;

#[utoipa::path(
    get,
    path = "/game/rooms",
    tag = "小游戏",
    summary = "小游戏大厅：获取房间列表（IM 群组列表）",
    responses((status = 200, body = ImRoomListOut), (status = 400, body = CustomError), (status = 401, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn list_rooms(
    _token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.as_ref().map(|c| (**c).clone()) else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };
    let client = ImRestClient::new(cfg);
    let groups = client.list_groups().await?;
    let rooms = groups
        .into_iter()
        .map(|(group_id, name, member_num, owner_identifier)| ImRoomOut {
            group_id,
            name,
            member_num,
            owner_identifier,
        })
        .collect();
    Ok(HttpResponse::Ok().json(&ImRoomListOut { rooms }))
}

#[utoipa::path(
    post,
    path = "/game/rooms",
    tag = "小游戏",
    summary = "创建房间（后端创建 IM 群组，并设置房主）",
    request_body = ImCreateRoomIn,
    responses((status = 200, body = ImRoomOut), (status = 400, body = CustomError), (status = 401, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn create_room(
    token: UserToken,
    body: Json<ImCreateRoomIn>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.as_ref().map(|c| (**c).clone()) else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };
    if body.name.trim().is_empty() {
        return Err(CustomError::BadRequest("房间名不能为空".into()));
    }

    let client = ImRestClient::new(cfg);
    let owner_identifier = token.user_id.to_string();

    // Ensure IM account exists
    let _ = client.account_import(&owner_identifier).await;

    let group_id = client.create_group(&owner_identifier, body.name.trim()).await?;
    Ok(HttpResponse::Ok().json(&ImRoomOut {
        group_id,
        name: body.name.trim().to_string(),
        member_num: Some(1),
        owner_identifier: Some(owner_identifier),
    }))
}

#[utoipa::path(
    post,
    path = "/game/rooms/{group_id}/join",
    tag = "小游戏",
    summary = "加入房间（后端将当前用户加入 IM 群组）",
    params(("group_id" = String, Path, description = "IM 群组 ID")),
    responses((status = 200, body = ImJoinRoomOut), (status = 400, body = CustomError), (status = 401, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn join_room(
    token: UserToken,
    group_id: ntex::web::types::Path<String>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.as_ref().map(|c| (**c).clone()) else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };
    let group_id = group_id.into_inner();
    if group_id.trim().is_empty() {
        return Err(CustomError::BadRequest("group_id 不能为空".into()));
    }
    let identifier = token.user_id.to_string();
    let client = ImRestClient::new(cfg);

    // Ensure IM account exists
    let _ = client.account_import(&identifier).await;

    // Idempotent join: if already in group, treat as success.
    let members = client.get_group_member_accounts(&group_id).await?;
    if !members.iter().any(|m| m == &identifier) {
        client.add_group_member(&group_id, &identifier).await?;
    }

    Ok(HttpResponse::Ok().json(&ImJoinRoomOut { group_id, identifier }))
}

#[utoipa::path(
    post,
    path = "/game/rooms/dismiss",
    tag = "小游戏",
    summary = "解散房间（解散 IM 群组，仅群主可操作）",
    request_body = ImDismissRoomIn,
    responses((status = 200, body = ImDismissRoomOut), (status = 400, body = CustomError), (status = 401, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn dismiss_room(
    token: UserToken,
    body: Json<ImDismissRoomIn>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.as_ref().map(|c| (**c).clone()) else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };

    let group_id = body.group_id.trim().to_string();
    if group_id.is_empty() {
        return Err(CustomError::BadRequest("GroupId 不能为空".into()));
    }

    let caller_identifier = token.user_id.to_string();
    let client = ImRestClient::new(cfg);

    let owner = client.get_group_owner_account(&group_id).await?;
    if owner.as_deref() != Some(&caller_identifier) {
        return Err(CustomError::BadRequest("仅房主可解散房间".into()));
    }

    client.destroy_group(&group_id).await?;
    Ok(HttpResponse::Ok().json(&ImDismissRoomOut { group_id }))
}
