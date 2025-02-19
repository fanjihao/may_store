use std::sync::Arc;

use ntex::web::{types::{Json, Query, State}, Responder};

use crate::{errors::CustomError, models::wishes::WishedListOut, AppState};

#[utoipa::path(
    get,
    path = "/wishes",
    tag = "心愿",
    responses(
        (status = 200, body = Vec<WishedListOut>, description = "获取心愿列表")
    )
)]
pub async fn all_wishes(
    state: State<Arc<AppState>>,
    data: Query<WishedListOut> 
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let rows = sqlx::query_as!(
        WishedListOut,
        "SELECT * FROM point_wish WHERE create_by = $1",
        data.create_by
    )
    .fetch_all(db_pool)
    .await?;

    Ok(Json(rows))
}