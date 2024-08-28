use std::sync::Arc;

use ntex::web::types::{Json, Query, State};

use crate::{errors::CustomError, models::{foods::{FoodApply, FoodApplyStruct}, users::UserToken}, AppState};


pub async fn apply_record(
    _: UserToken,
    data: Query<FoodApply>,
    state: State<Arc<AppState>>,
) -> Result<Json<Vec<FoodApplyStruct>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let food_by_status = sqlx::query!(
        "SELECT f.*, fc.class_name FROM foods f LEFT JOIN food_class fc ON fc.class_id = f.food_types WHERE f.food_status = $1 AND f.user_id = $2;",
        data.status,
        data.user_id
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