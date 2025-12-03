use std::sync::Arc;

use ntex::web::{types::{Json, State}, Responder, HttpResponse};

use crate::{errors::CustomError, models::users::Register, AppState};

#[utoipa::path(
    post,
    path = "/wx-register",
    request_body = Register,
    tag = "用户",
    responses(
        (status = 201, body = Register),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    )
)]
pub async fn wx_register(
    data: Json<Register>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "INSERT INTO users (account, password, nick_name, avatar) VALUES ($1, $2, $3, $4)",
        data.account,
        data.password,
        data.nick_name,
        data.avatar
    )
    .execute(db_pool)
    .await?;

    Ok(HttpResponse::Created().body("注册成功.".to_string()))
}