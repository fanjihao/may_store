use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    models::pagination::CursorPage,
    users::models::sweet_talk::{SweetTalkOut, SweetTalkQuery, SweetTalkRequest},
    users::models::user::UserToken,
    users::service::SweetTalkService,
};

#[utoipa::path(
    post,
    path = "/users/sweet-talk",
    tag = "用户",
    summary = "发表每日情话",
    request_body = SweetTalkRequest,
    responses(
        (status = 200, description = "发表成功"),
        (status = 400, description = "今日已发表或未绑定"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn add_sweet_talk(
    token: UserToken,
    data: Json<SweetTalkRequest>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let talk_id =
        SweetTalkService::add_sweet_talk(token.user_id, data.into_inner(), &state).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "talkId": talk_id,
        "message": "发表成功"
    })))
}

#[utoipa::path(
    put,
    path = "/users/sweet-talk/{id}",
    tag = "用户",
    summary = "编辑每日情话",
    request_body = SweetTalkRequest,
    params(
        ("id" = i64, Path, description = "情话 ID")
    ),
    responses(
        (status = 200, description = "修改成功"),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权编辑"),
        (status = 404, description = "情话不存在"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_sweet_talk(
    token: UserToken,
    id: Path<i64>,
    data: Json<SweetTalkRequest>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    SweetTalkService::update_sweet_talk(token.user_id, id.into_inner(), data.into_inner(), &state)
        .await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "message": "修改成功"
    })))
}

#[utoipa::path(
    get,
    path = "/users/sweet-talks",
    tag = "用户",
    summary = "获取情话历史",
    params(SweetTalkQuery),
    responses(
        (status = 200, body = CursorPage<SweetTalkOut>),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_sweet_talks(
    token: UserToken,
    query: Query<SweetTalkQuery>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = SweetTalkService::get_sweet_talks(token.user_id, query.into_inner(), &state).await?;
    Ok(HttpResponse::Ok().json(&res))
}
