use std::sync::Arc;
use ntex::web::types::{Json, Path, State};
use crate::{errors::CustomError, models::orders::OrderFootprint, AppState};

#[utoipa::path(
    get,
    path = "/footprints/{id}",
    tag = "足迹列表",
    responses(
        (status = 200, body = Vec<OrderFootprint>, description = "获取足迹列表")
    )
)]
pub async fn footprints_list(
    state: State<Arc<AppState>>,
    id: Path<(i32,)>,
) -> Result<Json<Vec<OrderFootprint>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let result = sqlx::query_as!(
        OrderFootprint,
        "SELECT *   
        FROM footprints
        WHERE ship_id = $1",
        id.0
    )
    .fetch_all(db_pool)
    .await?;

    Ok(Json(result))
}
