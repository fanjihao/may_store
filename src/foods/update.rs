use std::sync::Arc;

use ntex::web::{
    types::{Json, Path, State},
    HttpResponse, Responder,
};

use crate::{
    errors::CustomError,
    models::{foods::UpdateFood, users::UserToken},
    AppState,
};

pub async fn update_record(
    _: UserToken,
    data: Json<UpdateFood>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "UPDATE foods SET food_status = $2, apply_remarks = $3 WHERE food_id = $1",
        data.food_id,
        data.food_status,
        data.msg
    )
    .execute(db_pool)
    .await?;

    Ok(HttpResponse::Created().body("操作成功"))
}

pub async fn delete_record(
    _: UserToken,
    id: Path<(i32,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!("DELETE FROM foods WHERE food_id = $1", id.0)
        .execute(db_pool)
        .await?;

    Ok(HttpResponse::Created().body("删除成功"))
}
