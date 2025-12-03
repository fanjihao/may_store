use std::sync::Arc;

use ntex::web::{types::{Json, State}, Responder, HttpResponse};

use crate::{errors::CustomError, models::users::{UserInfo, UserToken}, AppState};

#[utoipa::path(
    post,
    path = "/users",
    operation_id = "wx_change_info",
    tag = "用户",
    request_body = UserInfo,
    responses(
        (status = 201, body = UserInfo),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn wx_change_info(
    _: UserToken,
    data: Json<UserInfo>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    if data.password.is_none() {
        if data.role.is_none() {
            sqlx::query!(
                "UPDATE users SET nick_name = $1, gender = $2, avatar = $3, birthday = $4 WHERE account = $5",
                data.nick_name,
                data.gender,
                data.avatar,
                data.birthday,
                data.account
            )
            .execute(db_pool)
            .await?;
        } else {
            sqlx::query!(
                "UPDATE users SET role = $1 WHERE account = $2",
                data.role,
                data.account
            )
            .execute(db_pool)
            .await?;
        }
    } else {
        sqlx::query!(
            "UPDATE users SET password = $1 WHERE account = $2",
            data.password,
            data.account
        )
        .execute(db_pool)
        .await?;
    }

    Ok(HttpResponse::Created().body("更新成功.".to_string()))
}