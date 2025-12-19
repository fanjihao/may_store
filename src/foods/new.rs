use crate::{
    errors::CustomError,
    models::{
        foods::{
            ApplyStatusEnum, FoodCreateInput, FoodOut, FoodRecord, FoodStatusEnum,
            FoodTagOut, SubmitRoleEnum, TagRecord,
        },
        users::UserToken,
    },
    AppState,
};
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
	post,
	path = "/foods",
	tag = "菜品",
	request_body = FoodCreateInput,
	responses((status = 201, body = FoodOut)),
	security(("cookie_auth" = []))
)]
pub async fn create_food(
    token: UserToken,
    data: Json<FoodCreateInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // 获取用户角色（简单获取）
    let role: Option<String> = sqlx::query_scalar("SELECT role::text FROM users WHERE user_id=$1")
        .bind(token.user_id as i64)
        .fetch_optional(&mut *tx)
        .await?;
    if role.is_none() {
        return Err(CustomError::BadRequest("用户不存在".into()));
    }
    let role = role.unwrap();

    // 业务：不同角色提交方式
    let submit_role = if role == "RECEIVING" {
        SubmitRoleEnum::RECEIVING_CREATE
    } else {
        SubmitRoleEnum::ORDERING_APPLY
    };
    let apply_status = if matches!(submit_role, SubmitRoleEnum::RECEIVING_CREATE) {
        ApplyStatusEnum::APPROVED
    } else {
        ApplyStatusEnum::PENDING
    };
    let food_status = if matches!(apply_status, ApplyStatusEnum::APPROVED) {
        FoodStatusEnum::NORMAL
    } else {
        FoodStatusEnum::AUDITING
    };

    let rec = sqlx::query_as::<_, FoodRecord>(
		"INSERT INTO foods (food_name, food_photo, ingredients, steps, submit_role, apply_status, food_status, created_by, owner_user_id, group_id, apply_remark, tag_id) \
		 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11) RETURNING food_id, food_name, food_photo, ingredients, steps, food_status, submit_role, apply_status, apply_remark, created_by, owner_user_id, group_id, approved_at, approved_by, is_del, created_at, updated_at, tag_id"
	)
	.bind(&data.food_name)
	.bind(&data.food_photo)
    .bind(&data.ingredients)
    .bind(&data.steps)
	.bind(submit_role)
	.bind(apply_status)
	.bind(food_status)
	.bind(token.user_id as i64)
	.bind(data.group_id.map(|v| v as i64))
	.bind(Option::<String>::None) // apply_remark
    .bind(data.tag_id)
	.fetch_one(&mut *tx)
	.await?;

    // 查询标签
    let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
        sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
            .bind(tid)
            .fetch_optional(&mut *tx)
            .await?
    } else {
        None
    };

    // 查询标记
    let marks: Vec<String> =
        sqlx::query("SELECT mark_type::text FROM user_food_mark WHERE user_id=$1 AND food_id=$2")
            .bind(token.user_id as i64)
            .bind(rec.food_id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect();
    let mark_enums = marks
        .into_iter()
        .filter_map(|s| match s.as_str() {
            "LIKE" => Some(crate::models::foods::MarkTypeEnum::LIKE),
            "NOT_RECOMMEND" => Some(crate::models::foods::MarkTypeEnum::NOT_RECOMMEND),
            _ => None,
        })
        .collect::<Vec<_>>();

    tx.commit().await?;
    let out = FoodOut::from((rec, tag_row, mark_enums));
    Ok(HttpResponse::Created().json(&out))
}

use crate::models::foods::TagCreateInput;

#[utoipa::path(
	post,
	path = "/food_tags",
	tag = "菜品",
	request_body = TagCreateInput,
	responses((status = 201, body = FoodTagOut)),
	security(("cookie_auth" = []))
)]
pub async fn create_tag(
    token: UserToken,
    data: Json<TagCreateInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let gid = data.group_id.or(token.user.as_ref().and_then(|u| u.group_id));
    let rec = sqlx::query_as::<_, TagRecord>(
		"INSERT INTO tags (tag_name, group_id, sort) VALUES ($1,$2,$3) RETURNING tag_id, tag_name, group_id, sort, created_at"
	)
	.bind(&data.tag_name)
    .bind(gid)
	.bind(data.sort)
	.fetch_one(db)
	.await?;
    Ok(HttpResponse::Created().json(&FoodTagOut {
        tag_id: rec.tag_id,
        tag_name: rec.tag_name,
    }))
}
