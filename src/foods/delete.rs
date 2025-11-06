use crate::{errors::CustomError, models::users::UserToken, AppState};
use ntex::web::{
    types::{Path, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

#[utoipa::path(
	delete,
	path = "/foods/{id}",
	tag = "菜品",
	params(("id"=i64, Path, description="菜品ID")),
	responses((status = 200, body = String)),
	security(("cookie_auth" = []))
)]
pub async fn delete_food(
    _token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<(i64,)>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    sqlx::query("UPDATE foods SET food_status='OFF', updated_at=NOW() WHERE food_id=$1")
        .bind(id.0)
        .execute(db)
        .await?;
    Ok(HttpResponse::Ok().body("deleted"))
}

#[utoipa::path(
	delete,
	path = "/food_tags/{id}",
	tag = "菜品",
	params(("id"=i64, Path, description="标签ID")),
	responses((status = 200, body = String)),
	security(("cookie_auth" = []))
)]
pub async fn delete_tag(
    _token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<(i64,)>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    sqlx::query("DELETE FROM food_tags_map WHERE tag_id=$1")
        .bind(id.0)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM tags WHERE tag_id=$1")
        .bind(id.0)
        .execute(db)
        .await?;
    Ok(HttpResponse::Ok().body("deleted"))
}
