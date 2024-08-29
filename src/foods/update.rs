use std::sync::Arc;

use ntex::web::{types::{Path, State}, Responder, HttpResponse};

use crate::{errors::CustomError, models::users::UserToken, AppState};



pub async fn revoke_record(
    _: UserToken,
    id: Path<(i32,)>,
    state: State<Arc<AppState>>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "UPDATE foods SET food_status = 2 WHERE food_id = $1",
        id.0
    ).execute(db_pool).await?;

    Ok(HttpResponse::Created().body("撤回成功"))
}

pub async fn delete_record(
    _: UserToken,
    id: Path<(i32,)>,
    state: State<Arc<AppState>>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "DELETE FROM foods WHERE food_id = $1", 
        id.0
    ).execute(db_pool).await?;

    Ok(HttpResponse::Created().body("删除成功"))
}