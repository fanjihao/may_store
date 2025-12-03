use std::sync::Arc;

use ntex::web::{types::{Path, State}, Responder, HttpResponse};

use crate::{errors::CustomError, AppState};

#[utoipa::path(
    delete,
    path = "/orders/{id}",
    params(
        ("id" = i32, Path, description = "订单ID")
    ),
    tag = "订单",
    responses(
        (status = 200, body = String, description = "删除成功")
    )
)]
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