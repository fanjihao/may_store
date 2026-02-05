use crate::{
    AppState, errors::CustomError, models::{foods::{
        FoodOut, FoodRecord, FoodTagOut, FoodUpdateInput, MarkTypeEnum, SubmitRoleEnum, TagRecord, TagUpdateInput, BatchTagSortInput
    }, users::UserToken}
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
    // - RECEIVING 自主创建（RECEIVING_CREATE）：允许团队成员或管理员修改
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

    // 检查是否为团队成员
    let is_team_member = if let Some(gid) = rec.group_id {
        let member: Option<i32> = sqlx::query_scalar("SELECT 1 FROM association_group_members WHERE user_id=$1 AND group_id=$2")
            .bind(uid)
            .bind(gid)
            .fetch_optional(&mut *tx)
            .await?;
        member.is_some()
    } else {
        false
    };

    let can_update = match rec.submit_role {
        SubmitRoleEnum::ReceivingCreate => is_admin || is_team_member,
        SubmitRoleEnum::OrderingApply => {
            is_admin || rec.created_by == uid || (is_receiving && rec.created_by != uid)
        }
    };
    if !can_update {
        return Err(CustomError::BadRequest("无权限修改".into()));
    }

    // 禁止自审：如果是 ORDERING 申请菜品，创建者本人不允许修改 apply_status。
    // （即使该用户通过“角色互换”切到 RECEIVING，也不能审核自己提交的申请。）
    if !is_admin
        && matches!(rec.submit_role, SubmitRoleEnum::OrderingApply)
        && rec.created_by == uid
        && data.apply_status.is_some()
    {
        return Err(CustomError::BadRequest("禁止自审".into()));
    }

    // 非管理员审核必须是 RECEIVING
    if !is_admin
        && matches!(rec.submit_role, SubmitRoleEnum::OrderingApply)
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
            "NOT_RECOMMEND" => Some(MarkTypeEnum::NotRecommend),
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
    sqlx::query("DELETE FROM user_food_mark WHERE user_id=$1 AND food_id=$2 AND mark_type=$3")
        .bind(token.user_id as i64)
        .bind(food_id)
        .bind(mark_type)
        .execute(db)
        .await?;
    Ok(HttpResponse::Ok().body("ok"))
}

#[utoipa::path(
	put,
	path = "/food_tags/{id}",
	tag = "标签",
	request_body = TagUpdateInput,
	params(("id" = i64, Path, description = "标签ID")),
	responses((status = 200, body = FoodTagOut)),
	security(("cookie_auth" = []))
)]
pub async fn update_tag(
    _token: UserToken,
    state: State<Arc<AppState>>,
    id: ntex::web::types::Path<(i64,)>,
    data: Json<TagUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    let tag_id = id.0;

    // Check if tag exists
    let rec_opt = sqlx::query_as::<_, TagRecord>("SELECT * FROM tags WHERE tag_id=$1 FOR UPDATE")
        .bind(tag_id)
        .fetch_optional(&mut *tx)
        .await?;

    let mut rec = match rec_opt {
        Some(r) => r,
        None => return Err(CustomError::BadRequest("标签不存在".into())),
    };

    if let Some(name) = &data.tag_name {
        rec.tag_name = name.clone();
    }
    if let Some(icon) = &data.icon {
        rec.icon = Some(icon.clone());
    }
    if let Some(sort) = data.sort {
        rec.sort = Some(sort);
    }

    sqlx::query("UPDATE tags SET tag_name=$2, icon=$3, sort=$4 WHERE tag_id=$1")
        .bind(rec.tag_id)
        .bind(&rec.tag_name)
        .bind(&rec.icon)
        .bind(rec.sort)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(HttpResponse::Ok().json(&FoodTagOut {
        tag_id: rec.tag_id,
        tag_name: rec.tag_name,
        icon: rec.icon,
        sort: rec.sort,
        food_count: None,
    }))
}

#[utoipa::path(
	post,
	path = "/food_tags/sort",
	tag = "标签",
	request_body = BatchTagSortInput,
	responses((status = 200, body = String)),
	security(("cookie_auth" = []))
)]
pub async fn update_tags_sort(
    _token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<BatchTagSortInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let items = &data.items;

    if items.is_empty() {
        return Ok(HttpResponse::Ok().body("ok"));
    }

    let mut tx = db.begin().await?;

    // Build the query dynamically
    // UPDATE tags SET sort = CASE tag_id
    //   WHEN 1 THEN 10
    //   WHEN 2 THEN 20
    //   ELSE sort END
    // WHERE tag_id IN (1, 2)

    let mut ids = Vec::new();

    // Using numbered parameters $1, $2, etc. is tricky with variable length.
    // However, since we are iterating, we can construct the query string with placeholders.
    // The bind order must match.

    // Current implementation with sqlx QueryBuilder is cleaner for dynamic queries,
    // but a simple string construction with parameterized query is also fine for this specific case.

    // Let's use QueryBuilder for safety and convenience
    use sqlx::QueryBuilder;

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("UPDATE tags SET sort = CASE tag_id ");

    for item in items {
        qb.push("WHEN ");
        qb.push_bind(item.tag_id);
        qb.push(" THEN ");
        qb.push_bind(item.sort);
        qb.push(" ");
        ids.push(item.tag_id);
    }

    qb.push("ELSE sort END WHERE tag_id IN (");

    let mut separated = qb.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    qb.build().execute(&mut *tx).await?;

    tx.commit().await?;

    Ok(HttpResponse::Ok().body("ok"))
}
