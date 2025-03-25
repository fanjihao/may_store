use std::sync::Arc;

use ntex::web::{
    types::{Json, Path, State},
    HttpResponse, Responder,
};

use crate::{
    errors::CustomError,
    models::{foods::UpdateFood, users::UserToken},
    AppState,
};

#[utoipa::path(
    put,
    path = "/food/update",
    request_body = UpdateFood,
    tag = "菜品",
    responses(
        (status = 201, body = String, description = "操作成功"),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn update_record(
    _: UserToken,
    data: Json<UpdateFood>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "UPDATE foods SET food_status = $2, apply_remarks = $3 WHERE food_id = $1",
        data.food_id,
        data.food_status,
        data.msg
    )
    .execute(db_pool)
    .await?;

    Ok(HttpResponse::Created().body("操作成功"))
}

#[utoipa::path(
    delete,
    path = "/food/delete/{id}",
    params(
        ("id" = i32, Path, description = "菜品ID")
    ),
    tag = "菜品",
    responses(
        (status = 201, body = String, description = "删除成功"),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn delete_record(
    _: UserToken,
    id: Path<(i32,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!("DELETE FROM foods WHERE food_id = $1", id.0)
        .execute(db_pool)
        .await?;

    Ok(HttpResponse::Created().body("删除成功"))
}

#[utoipa::path(
    put,
    path = "/food/mark/{id}",
    params(
        ("id" = i32, Path, description = "菜品ID")
    ),
    tag = "菜品",
    responses(
        (status = 201, body = String, description = "操作成功"),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn favorite_dishes(
    id: Path<(i32,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    sqlx::query!(
        "UPDATE foods SET is_mark = CASE WHEN is_mark = 1 THEN 0 ELSE 1 END WHERE food_id = $1",
        id.0
    )
    .execute(db_pool)
    .await?;

    Ok(HttpResponse::Created().body("操作成功"))
}

#[utoipa::path(
    delete,
    path = "/foodtag/{id}",
    params(
        ("id" = i32, Path, description = "标签ID")
    ),
    tag = "菜品",
    responses(
        (status = 201, body = String, description = "删除成功"),
        (status = 400, body = CustomError, example = json!(CustomError::BadRequest("参数错误".to_string())))
    ),
    security(
        ("cookie_auth" = [])
    )
)]
pub async fn delete_tags(
    _: UserToken,
    id: Path<(i32,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    println!("id: {:?}", id.0);
    sqlx::query!("DELETE FROM food_tags WHERE tag_id = $1", id.0)
        .execute(db_pool)
        .await?;

    Ok(HttpResponse::Created().body("删除成功"))
}