use crate::{
    errors::CustomError,
    models::foods::{FoodOut, FoodRecord, FoodUpdateInput, MarkTypeEnum, SubmitRoleEnum, TagRecord},
    models::users::UserToken,
    AppState,
};
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
	put,
	path = "/foods/{id}",
	tag = "菜品",
	request_body = FoodUpdateInput,
	params(("id" = i64, Path, description = "菜品ID")),
	responses((status = 200, body = FoodOut)),
	security(("cookie_auth" = []))
)]
pub async fn update_food(
    token: UserToken,
    state: State<Arc<AppState>>,
    id: ntex::web::types::Path<(i64,)>,
    data: Json<FoodUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;
    // 获取现有记录
    let rec_opt = sqlx::query_as::<_, FoodRecord>(
		"SELECT food_id, food_name, food_photo, ingredients, steps, food_status, submit_role, apply_status, apply_remark, created_by, owner_user_id, group_id, approved_at, approved_by, is_del, created_at, updated_at, tag_id FROM foods WHERE food_id=$1 FOR UPDATE"
	)
	.bind(id.0)
	.fetch_optional(&mut *tx)
	.await?;
    let mut rec = match rec_opt {
        Some(r) => r,
        None => return Err(CustomError::BadRequest("菜品不存在".into())),
    };

    // 权限：
    // - RECEIVING 自主创建（RECEIVING_CREATE）：仅允许本人或管理员修改
    // - ORDERING 申请（ORDERING_APPLY）：允许 RECEIVING 审核，但禁止自审（同一账号切角色也不行）
    let role: Option<String> = sqlx::query_scalar("SELECT role::text FROM users WHERE user_id=$1")
        .bind(token.user_id as i64)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(role) = role else {
        return Err(CustomError::BadRequest("用户不存在".into()));
    };
    let is_admin = role == "ADMIN";
    let is_receiving = role == "RECEIVING";
    let uid = token.user_id as i64;

    let can_update = match rec.submit_role {
        SubmitRoleEnum::RECEIVING_CREATE => is_admin || rec.created_by == uid,
        SubmitRoleEnum::ORDERING_APPLY => is_admin || rec.created_by == uid || (is_receiving && rec.created_by != uid),
    };
    if !can_update {
        return Err(CustomError::BadRequest("无权限修改".into()));
    }

    // 禁止自审：如果是 ORDERING 申请菜品，创建者本人不允许修改 apply_status。
    // （即使该用户通过“角色互换”切到 RECEIVING，也不能审核自己提交的申请。）
    if !is_admin
        && matches!(rec.submit_role, SubmitRoleEnum::ORDERING_APPLY)
        && rec.created_by == uid
        && data.apply_status.is_some()
    {
        return Err(CustomError::BadRequest("禁止自审".into()));
    }

    // 非管理员审核必须是 RECEIVING
    if !is_admin
        && matches!(rec.submit_role, SubmitRoleEnum::ORDERING_APPLY)
        && data.apply_status.is_some()
        && !is_receiving
    {
        return Err(CustomError::BadRequest("仅接单角色可审核".into()));
    }

    // 应用更新字段
    if let Some(name) = &data.food_name {
        rec.food_name = name.clone();
    }
    if let Some(photo) = &data.food_photo {
        rec.food_photo = Some(photo.clone());
    }
    if let Some(ing) = &data.ingredients {
        rec.ingredients = Some(ing.clone());
    }
    if let Some(st) = &data.steps {
        rec.steps = Some(st.clone());
    }
    if let Some(r) = &data.apply_remark {
        rec.apply_remark = Some(r.clone());
    }
    if let Some(status) = data.food_status {
        rec.food_status = status;
    }
    if let Some(app_status) = data.apply_status {
        rec.apply_status = app_status;
    }

    if let Some(tid) = data.tag_id {
        rec.tag_id = Some(tid);
    }

    sqlx::query(
		"UPDATE foods SET food_name=$2, food_photo=$3, ingredients=$4, steps=$5, apply_remark=$6, food_status=$7, apply_status=$8, tag_id=$9, updated_at=NOW() WHERE food_id=$1"
	)
	.bind(rec.food_id)
	.bind(&rec.food_name)
	.bind(&rec.food_photo)
    .bind(&rec.ingredients)
    .bind(&rec.steps)
	.bind(&rec.apply_remark)
	.bind(rec.food_status)
	.bind(rec.apply_status)
    .bind(rec.tag_id)
	.execute(&mut *tx)
	.await?;

    let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
        sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
            .bind(tid)
            .fetch_optional(&mut *tx)
            .await?
    } else {
        None
    };

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
            "LIKE" => Some(MarkTypeEnum::LIKE),
            "NOT_RECOMMEND" => Some(MarkTypeEnum::NOT_RECOMMEND),
            _ => None,
        })
        .collect::<Vec<_>>();
    tx.commit().await?;

    Ok(HttpResponse::Ok().json(&FoodOut::from((rec, tag_row, mark_enums))))
}

use crate::models::foods::FoodMarkActionInput;

#[utoipa::path(
	post,
	path = "/foods/mark",
	tag = "菜品",
	request_body = FoodMarkActionInput,
	responses((status = 200, body = String)),
	security(("cookie_auth" = []))
)]
pub async fn mark_food(
    token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<FoodMarkActionInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    sqlx::query("INSERT INTO user_food_mark (user_id, food_id, mark_type) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
		.bind(token.user_id as i64)
		.bind(data.food_id)
		.bind(data.mark_type)
		.execute(db)
		.await?;
    Ok(HttpResponse::Ok().body("ok"))
}

#[utoipa::path(
	delete,
	path = "/foods/mark/{food_id}/{mark_type}",
	tag = "菜品",
	params(("food_id"=i64, Path), ("mark_type"=MarkTypeEnum, Path)),
	responses((status = 200, body = String)),
	security(("cookie_auth" = []))
)]
pub async fn unmark_food(
    token: UserToken,
    state: State<Arc<AppState>>,
    path: ntex::web::types::Path<(i64, MarkTypeEnum)>,
) -> Result<impl Responder, CustomError> {
    let (food_id, mark_type) = (path.0, path.1);
    let db = &state.db_pool;
    sqlx::query(
        "DELETE FROM user_food_mark WHERE user_id=$1 AND food_id=$2 AND mark_type=$3",
    )
    .bind(token.user_id as i64)
    .bind(food_id)
    .bind(mark_type)
    .execute(db)
    .await?;
    Ok(HttpResponse::Ok().body("ok"))
}
