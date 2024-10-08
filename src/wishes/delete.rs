use std::sync::Arc;

use ntex::web::{types::{Path, State}, HttpResponse, Responder};

use crate::{errors::CustomError, AppState};


pub async fn delete_wishes(
    state: State<Arc<AppState>>,
    id: Path<(i32,)>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "DELETE FROM point_wish WHERE id = $1", 
        id.0
    ).execute(db_pool).await?;

    Ok(HttpResponse::Created().body("删除成功"))
}