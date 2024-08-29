use std::sync::Arc;

use ntex::web::{types::{Json, Query, State}, Responder};

use crate::{errors::CustomError, models::{foods::{FoodApply, FoodApplyStruct, ShowClass}, users::UserToken}, AppState};


pub async fn apply_record(
    _: UserToken,
    data: Query<FoodApply>,
    state: State<Arc<AppState>>,
) -> Result<Json<Vec<FoodApplyStruct>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let user = sqlx::query!("SELECT u.* FROM users u WHERE u.user_id=$1", data.user_id)
        .fetch_one(db_pool)
        .await?;

    let mut user_ids: Vec<i32> = Vec::new();
    // if user.user_id == 1 {
        user_ids.push(user.user_id);
    // } else {
    //     user_ids.push(user.user_id);
    //     user_ids.push(user.role);
    // }

    let status: Vec<i32> = data.status.split(",").map(|s| s.trim().parse().unwrap()).collect();
    let food_by_status = sqlx::query!(
        "SELECT f.*, fc.class_name FROM foods f LEFT JOIN food_class fc ON fc.class_id = f.food_types 
        WHERE f.food_status = ANY($1::int[]) AND f.user_id = ANY($2::int[]) ORDER BY f.create_time DESC;",
        &status,
        &user_ids
    )
    .fetch_all(db_pool)
    .await?
    .iter()
    .map(|i| FoodApplyStruct {
        food_id: Some(i.food_id),
        food_name: i.food_name.clone().unwrap_or_default(),
        food_photo: i.food_photo.clone(),
        food_tags: i.food_tags.clone(),
        food_types: i.food_types.clone(),
        class_name: i.class_name.clone(),
        food_status: i.food_status,
        food_reason: i.food_reason.clone(),
        create_time: i.create_time,
        finish_time: i.finish_time,
        is_del: i.is_del,
        is_mark: i.is_mark,
        user_id: i.user_id,
        apply_remarks: i.apply_remarks.clone(),
    })
    .collect::<Vec<FoodApplyStruct>>();

    Ok(Json(food_by_status))
}

pub async fn all_food_class(
    _: UserToken,
    c: Query<ShowClass>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let user_id = c.user_id.unwrap_or_default() as i32;
    let class = sqlx::query_as!(ShowClass,
        "SELECT f.* FROM food_class f LEFT JOIN users u ON f.user_id = u.user_id WHERE u.user_id = $1",
        user_id
    )
    .fetch_all(db_pool)
    .await?;

    Ok(Json(class))
}