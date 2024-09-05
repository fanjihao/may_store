use std::sync::Arc;

use crate::{
    errors::CustomError,
    models::{
        foods::{DishesByType, FoodApply, FoodApplyStruct, FoodTags, ShowClass},
        users::UserToken,
    },
    AppState,
};
use ntex::web::{
    types::{Json, Path, Query, State},
    Responder,
};
use sqlx::Row;

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

    let status: Vec<i32> = data
        .status
        .split(",")
        .map(|s| s.trim().parse().unwrap())
        .collect();
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

pub async fn get_foods(
    data: Json<DishesByType>,
    state: State<Arc<AppState>>,
) -> Result<Json<Vec<FoodApplyStruct>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let mut sql = String::from("SELECT * FROM foods WHERE (user_id = $1");

    if let Some(a_id) = data.associate_id {
        sql.push_str(" OR user_id = ");
        sql.push_str(&a_id.to_string());
    }

    if let Some(mark) = data.is_mark {
        sql.push_str(") AND is_mark = ");
        sql.push_str(&mark.to_string());
    } else if let Some(types) = data.food_types {
        sql.push_str(") AND food_types = ");
        sql.push_str(&types.to_string());
    }

    sql.push_str(" AND food_status IN (0, 3)");

    if let Some(tags_str) = &data.tags {
        let tags_list: Vec<&str> = tags_str.split(',').collect();
        let like_conditions: Vec<String> = tags_list
            .iter()
            .map(|tag| format!("food_tags LIKE '%{}%'", tag))
            .collect();

        let like_query = like_conditions.join(" OR ");
        sql.push_str(" AND (");
        sql.push_str(&like_query);
        sql.push(')');
    }
    println!("sql: ---{}", sql);
    let row = sqlx::query(&sql)
        .bind(data.user_id)
        .fetch_all(db_pool)
        .await?;

    let foods: Vec<FoodApplyStruct> = row
        .into_iter()
        .map(|row| FoodApplyStruct {
            food_id: row.get("food_id"),
            food_name: row.get("food_name"),
            food_photo: row.get("food_photo"),
            class_name: None,
            food_reason: row.get("food_reason"),
            create_time: row.get("create_time"),
            finish_time: row.get("finish_time"),
            is_mark: row.get("is_mark"),
            is_del: row.get("is_del"),
            user_id: row.get("user_id"),
            food_status: row.get("food_status"),
            food_types: row.get("food_types"),
            food_tags: row.get("food_tags"),
            apply_remarks: row.get("apply_remarks"),
        })
        .collect();

    Ok(Json(foods))
}

pub async fn get_tags(data: Query<DishesByType>, state: State<Arc<AppState>>) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let records = sqlx::query_as!(
        FoodTags,
        "SELECT *
        FROM food_tags
        WHERE (user_id = $1 OR user_id = $2)",
        data.user_id,
        data.associate_id
    ).fetch_all(db_pool).await?;

    Ok(Json(records))
}
