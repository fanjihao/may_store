use std::sync::Arc;

use ntex::web::{types::{Json, State}, Responder, HttpResponse};

use crate::{errors::CustomError, models::wishes::WishedListOut, AppState};

#[utoipa::path(
    post,
    path = "/wishes",
    request_body = WishedListOut,
    tag = "心愿",
    responses(
        (status = 200, body = String, description = "添加成功")
    )
)]
pub async fn new_wishes(
    state: State<Arc<AppState>>,
    data: Json<WishedListOut>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "INSERT INTO point_wish (wish_name, wish_cost, create_by) 
            VALUES ($1, $2, $3)",
        data.wish_name,
        data.wish_cost,
        data.create_by
    ).execute(db_pool).await?;

    Ok(HttpResponse::Created().body("添加成功"))
}