use std::sync::Arc;

use ntex::web::{types::{Json, State}, Responder, HttpResponse};
use crate::{errors::CustomError, models::{foods::NewFood, users::UserToken}, AppState};

#[utoipa::path(
    post,
    path = "/food/apply",
    tag = "菜品",
    request_body = NewFood,
    responses(
        (status = 200, body = String),
        (status = 400, body = CustomError)
    )
)]
pub async fn new_food_apply(
    _: UserToken,
    data: Json<NewFood>,
    state: State<Arc<AppState>>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "INSERT INTO foods (food_name, food_photo, food_tags, food_types, food_reason, user_id, food_status) 
            VALUES ($1, $2, $3, $4, $5, $6, $7)",
        data.food_name,
        data.food_photo,
        data.food_tags,
        data.food_types,
        data.food_reason,
        data.user_id,
        data.food_status
    ).execute(db_pool).await?;

    Ok(HttpResponse::Created().body("申请成功"))
}