use crate::errors::CustomError;
use crate::models::users::UserToken;
use crate::AppState;
use ntex::web::{
    types::{Path, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
	delete,
	path = "/orders/{id}",
	tag = "订单",
	params(("id" = i64, Path, description = "订单ID")),
	responses((status = 200, description = "订单删除成功"))
)]
pub async fn delete_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    // 检查订单是否存在且属于当前用户
    let row = sqlx::query("SELECT user_id FROM orders WHERE order_id=$1")
        .bind(*id)
        .fetch_optional(db)
        .await?;

    match row {
        Some(r) => {
            let order_user_id = r.get::<i64, _>("user_id");
            if order_user_id != user_token.user_id {
                return Err(CustomError::BadRequest("只能删除自己创建的订单".into()));
            }
        }
        None => {
            return Err(CustomError::BadRequest("订单不存在".into()));
        }
    }

    // 直接删除订单
    sqlx::query("DELETE FROM orders WHERE order_id=$1")
        .bind(*id)
        .execute(db)
        .await?;

    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "description": "订单删除成功"
    })))
}
