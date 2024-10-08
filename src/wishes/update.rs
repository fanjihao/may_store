use std::sync::Arc;

use ntex::web::{types::{Json, State}, HttpResponse, Responder};

use crate::{errors::CustomError, models::wishes::{WishCostDto, WishedListOut}, orders::update::log_points_transaction, AppState};


pub async fn clock_in_wish(
    state: State<Arc<AppState>>,
    data: Json<WishedListOut>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "UPDATE point_wish SET wish_location = $1, wish_date = $2, wish_photo = $3, mood = $4 WHERE id = $5",
        data.wish_location,
        data.wish_date,
        data.wish_photo,
        data.mood,
        data.id
    ).execute(db_pool).await?;

    Ok(HttpResponse::Created().body("更新成功"))
}

pub async fn update_wish_status(
    state: State<Arc<AppState>>,
    data: Json<WishCostDto>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut transaction = db_pool.begin().await?;

    let user = sqlx::query!(
        "SELECT love_point FROM users WHERE user_id = $1",
        data.user_id
    )
    .fetch_one(&mut *transaction)
    .await?;

    let row = sqlx::query!(
        "SELECT * FROM point_wish WHERE id = $1",
        data.id
    ).fetch_one(&mut *transaction).await?;

    sqlx::query!(
        "UPDATE point_wish SET exchange_status = 1 WHERE id = $1",
        data.id
    ).execute(&mut *transaction).await?;

    let balance = user.love_point.unwrap_or(0) - row.wish_cost.unwrap();
    let transaction_type = "redeem";

    let description = "解锁心愿";

    log_points_transaction(
        &mut transaction, 
        data.user_id.unwrap(),
        row.wish_cost.unwrap(),
        transaction_type,
        balance,
        description,
        data.id
    ).await?;

    transaction.commit().await?;

    Ok(HttpResponse::Created().body("更新成功"))
}