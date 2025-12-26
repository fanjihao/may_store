use std::sync::Arc;

use ntex::web::types::State;
use ntex::web::{HttpResponse, Responder};

use crate::errors::CustomError;
use crate::game_im::rest::ImRestClient;
use crate::models::game_im::{ImRoomListOut, ImRoomOut};
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
