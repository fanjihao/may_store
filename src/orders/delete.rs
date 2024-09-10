use std::sync::Arc;

use ntex::web::{types::{Path, State}, Responder, HttpResponse};

use crate::{errors::CustomError, AppState};

pub async fn delete_order(
    id: Path<(i32,)>,
    state: State<Arc<AppState>>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "UPDATE orders SET is_del = 1 WHERE order_id = $1",
        id.0
    )
    .execute(db_pool)
    .await?;

    Ok(HttpResponse::Created().body("删除成功"))
}