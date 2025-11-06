use crate::{
    errors::CustomError,
    models::{
        users::UserToken,
        wishes::{WishCreateInput, WishOut, WishRecord, WishStatusEnum},
    },
    AppState,
};
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(post, path="/wishes", tag="心愿", request_body=WishCreateInput, responses((status=201, body=WishOut)))]
pub async fn create_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<WishCreateInput>,
) -> Result<impl Responder, CustomError> {
    if data.wish_name.trim().is_empty() {
        return Err(CustomError::BadRequest("心愿名称不能为空".into()));
    }
    if data.wish_cost <= 0 {
        return Err(CustomError::BadRequest("心愿积分必须大于0".into()));
    }
    let db = &state.db_pool;
    let row = sqlx::query("INSERT INTO wishes (wish_name, wish_cost, created_by) VALUES ($1,$2,$3) RETURNING wish_id, wish_name, wish_cost, status, created_by, created_at, updated_at")
        .bind(&data.wish_name)
        .bind(data.wish_cost)
        .bind(user_token.user_id)
        .fetch_one(db).await?;
    let rec = WishRecord {
        wish_id: row.get("wish_id"),
        wish_name: row.get("wish_name"),
        wish_cost: row.get("wish_cost"),
        status: WishStatusEnum::ON,
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    Ok(HttpResponse::Created().json(&WishOut::from(rec)))
}
