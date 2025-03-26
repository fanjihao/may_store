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
    types::{Json, Query, State},
    Responder,
};
use sqlx::Row;

#[utoipa::path(
    get,
    path = "/food/records",
    params(
        ("user_id" = i32, Query, description = "用户ID"),
        ("status" = String, Query, description = "状态列表，用逗号分隔")
    ),
    tag = "菜品",
    responses(
        (status = 200, body = Vec<FoodApplyStruct>),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
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
        last_order_time: None,
        last_complete_time: None,
        total_order_count: None,
        completed_order_count: None
    })
    .collect::<Vec<FoodApplyStruct>>();

    Ok(Json(food_by_status))
}

#[utoipa::path(
    get,
    path = "/foodclass",
    params(
        ("user_id" = Option<i32>, Query, description = "用户ID")
    ),
    tag = "菜品",
    responses(
        (status = 200, body = Vec<ShowClass>),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn all_food_class(
    _: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let class = sqlx::query_as!(ShowClass,
        "SELECT f.* FROM food_class f",
    )
    .fetch_all(db_pool)
    .await?;

    Ok(Json(class))
}

#[utoipa::path(
    post,
    path = "/dishes",
    request_body = DishesByType,
    tag = "菜品",
    responses(
        (status = 200, body = Vec<FoodApplyStruct>),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    )
)]
pub async fn get_foods(
    data: Json<DishesByType>,
    state: State<Arc<AppState>>,
) -> Result<Json<Vec<FoodApplyStruct>>, CustomError> {
    let db_pool = &state.clone().db_pool;

    let mut sql = String::from(
        "SELECT 
            f.*,
            MAX ( o.create_date ) AS last_order_time,
            MAX ( o.finish_time ) AS last_complete_time,
            COUNT ( od.order_id ) AS total_order_count,
            SUM ( CASE WHEN o.order_status = 3 THEN 1 ELSE 0 END ) AS completed_order_count
        FROM 
            foods f
            LEFT JOIN orders_d od ON f.food_id = od.food_id
            LEFT JOIN orders o ON od.order_id = o.order_id
        WHERE (f.user_id = $1",
    );

    if let Some(a_id) = data.associate_id {
        sql.push_str(" OR f.user_id = ");
        sql.push_str(&a_id.to_string());
    }

    if let Some(mark) = data.is_mark {
        sql.push_str(") AND f.is_mark = ");
        sql.push_str(&mark.to_string());
    } else if let Some(types) = data.food_types {
        sql.push_str(") AND f.food_types = ");
        sql.push_str(&types.to_string());
    }

    sql.push_str(" AND f.food_status IN (0, 3)");

    if let Some(tags_str) = &data.tags {
        let tags_list: Vec<&str> = tags_str.split(',').collect();
        let like_conditions: Vec<String> = tags_list
            .iter()
            .map(|tag| format!("f.food_tags LIKE '%{}%'", tag))
            .collect();

        let like_query = like_conditions.join(" OR ");
        sql.push_str(" AND (");
        sql.push_str(&like_query);
        sql.push(')');
    }
    sql.push_str(" GROUP BY f.food_id ORDER BY last_order_time DESC;");
    // println!("sql: ---{}", sql);
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
            last_order_time: row.get("last_order_time"),
            last_complete_time: row.get("last_complete_time"),
            total_order_count: row.get("total_order_count"),
            completed_order_count: row.get("completed_order_count"),
        })
        .collect();

    Ok(Json(foods))
}

#[utoipa::path(
    get,
    path = "/foodtag",
    params(
        ("user_id" = i32, Query, description = "用户ID"),
        ("associate_id" = Option<i32>, Query, description = "关联用户ID")
    ),
    tag = "菜品",
    responses(
        (status = 200, body = Vec<FoodTags>),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn get_tags(
    data: Query<DishesByType>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    let records = sqlx::query_as!(
        FoodTags,
        "SELECT *
        FROM food_tags
        WHERE (user_id = $1 OR user_id = $2) ORDER BY sort ASC",
        data.user_id,
        data.associate_id
    )
    .fetch_all(db_pool)
    .await?;

    Ok(Json(records))
}
