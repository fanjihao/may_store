use crate::{
    errors::CustomError,
    models::foods::{
        BlindBoxDrawInput, BlindBoxDrawResultOut, BlindBoxFoodSnapshot, FoodCategory,
        FoodFilterQuery, FoodOut, FoodRecord, FoodTagOut, MarkTypeEnum, TagRecord,
    },
    models::users::UserToken,
    AppState,
};
use ntex::web::{
    types::{Path, Query, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
	get,
	path = "/foods",
	tag = "菜品",
	params(
		("keyword"=Option<String>, Query),
		("category"=Option<i32>, Query, description="类别 1-5"),
		("food_status"=Option<String>, Query),
		("apply_status"=Option<String>, Query),
		("tag_ids"=Option<String>, Query, description="以逗号分隔标签ID"),
		("group_id"=Option<i64>, Query)
	),
	responses((status = 200, body = Vec<FoodOut>))
)]
pub async fn get_foods(
    state: State<Arc<AppState>>,
    token: Option<UserToken>,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 动态条件构建（仅处理 keyword / category / group_id）
    enum BindValue {
        Text(String),
        I16(i16),
        I64(i64),
    }
    let mut conditions: Vec<String> = Vec::new();
    let mut binds: Vec<BindValue> = Vec::new();
    let mut param_index: usize = 1;
    if let Some(kw) = &q.keyword {
        conditions.push(format!("food_name ILIKE '%' || ${} || '%'", param_index));
        binds.push(BindValue::Text(kw.clone()));
        param_index += 1;
    }
    if let Some(cat) = q.category {
        conditions.push(format!("food_types = ${}", param_index));
        binds.push(BindValue::I16(cat as i16));
        param_index += 1;
    }
    if let Some(gid) = q.group_id {
        conditions.push(format!("group_id = ${}", param_index));
        binds.push(BindValue::I64(gid as i64));
        param_index += 1;
    }
    conditions.push("is_del = 0".into());
    let where_sql = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let base_sql = format!("SELECT food_id, food_name, food_photo, food_types, food_status, submit_role, apply_status, apply_remark, created_by, owner_user_id, group_id, approved_at, approved_by, is_del, created_at, updated_at FROM foods {} ORDER BY created_at DESC LIMIT 100", where_sql);
    let mut query = sqlx::query_as::<_, FoodRecord>(&base_sql);
    for b in binds {
        query = match b {
            BindValue::Text(v) => query.bind(v),
            BindValue::I16(v) => query.bind(v),
            BindValue::I64(v) => query.bind(v),
        };
    }
    let rows = query.fetch_all(db).await?;

    let mut out_list: Vec<FoodOut> = Vec::new();
    for rec in rows {
        let tag_rows: Vec<TagRecord> = sqlx::query_as("SELECT t.tag_id, t.tag_name, t.sort, t.created_at FROM tags t JOIN food_tags_map m ON t.tag_id=m.tag_id WHERE m.food_id=$1")
			.bind(rec.food_id)
			.fetch_all(db)
			.await?;
        let marks: Vec<String> = if let Some(t) = &token {
            sqlx::query(
                "SELECT mark_type::text FROM user_food_mark WHERE user_id=$1 AND food_id=$2",
            )
            .bind(t.user_id as i64)
            .bind(rec.food_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect()
        } else {
            Vec::new()
        };
        let mark_enums = marks
            .into_iter()
            .filter_map(|s| match s.as_str() {
                "LIKE" => Some(MarkTypeEnum::LIKE),
                "NOT_RECOMMEND" => Some(MarkTypeEnum::NOT_RECOMMEND),
                _ => None,
            })
            .collect();
        out_list.push(FoodOut::from((rec, tag_rows, mark_enums)));
    }
    Ok(HttpResponse::Ok().json(&out_list))
}

#[utoipa::path(
	get,
	path = "/foods/{id}",
	tag = "菜品",
	params(("id"=i64, Path)),
	responses((status = 200, body = FoodOut))
)]
pub async fn get_food_detail(
    state: State<Arc<AppState>>,
    token: Option<UserToken>,
    id: Path<(i64,)>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let rec_opt = sqlx::query_as::<_, FoodRecord>(
		"SELECT food_id, food_name, food_photo, food_types, food_status, submit_role, apply_status, apply_remark, created_by, owner_user_id, group_id, approved_at, approved_by, is_del, created_at, updated_at FROM foods WHERE food_id=$1"
	)
	.bind(id.0)
	.fetch_optional(db)
	.await?;
    let rec = match rec_opt {
        Some(r) => r,
        None => return Err(CustomError::BadRequest("未找到菜品".into())),
    };
    let tag_rows: Vec<TagRecord> = sqlx::query_as("SELECT t.tag_id, t.tag_name, t.sort, t.created_at FROM tags t JOIN food_tags_map m ON t.tag_id=m.tag_id WHERE m.food_id=$1")
		.bind(rec.food_id)
		.fetch_all(db)
		.await?;
    let marks: Vec<String> = if let Some(t) = token {
        sqlx::query("SELECT mark_type::text FROM user_food_mark WHERE user_id=$1 AND food_id=$2")
            .bind(t.user_id as i64)
            .bind(rec.food_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect()
    } else {
        Vec::new()
    };
    let mark_enums = marks
        .into_iter()
        .filter_map(|s| match s.as_str() {
            "LIKE" => Some(MarkTypeEnum::LIKE),
            "NOT_RECOMMEND" => Some(MarkTypeEnum::NOT_RECOMMEND),
            _ => None,
        })
        .collect();
    Ok(HttpResponse::Ok().json(&FoodOut::from((rec, tag_rows, mark_enums))))
}

#[utoipa::path(
	get,
	path = "/food_tags",
	tag = "菜品",
	responses((status = 200, body = Vec<FoodTagOut>))
)]
pub async fn get_tags(state: State<Arc<AppState>>) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let rows: Vec<TagRecord> = sqlx::query_as(
        "SELECT tag_id, tag_name, sort, created_at FROM tags ORDER BY sort NULLS LAST, tag_id",
    )
    .fetch_all(db)
    .await?;
    Ok(HttpResponse::Ok().json(
        &rows
            .into_iter()
            .map(|r| FoodTagOut {
                tag_id: r.tag_id,
                tag_name: r.tag_name,
            })
            .collect::<Vec<_>>(),
    ))
}

#[utoipa::path(
	get,
	path = "/foods/marks",
	tag = "菜品",
	responses((status = 200, body = Vec<FoodOut>)),
	security(("cookie_auth" = []))
)]
pub async fn get_marked_foods(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let rows: Vec<FoodRecord> = sqlx::query_as(
		"SELECT f.food_id, f.food_name, f.food_photo, f.food_types, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at \
		 FROM foods f JOIN user_food_mark m ON f.food_id=m.food_id WHERE m.user_id=$1 AND m.mark_type='LIKE'"
	)
	.bind(token.user_id as i64)
	.fetch_all(db)
	.await?;
    let mut out_list = Vec::new();
    for rec in rows {
        let tag_rows: Vec<TagRecord> = sqlx::query_as("SELECT t.tag_id, t.tag_name, t.sort, t.created_at FROM tags t JOIN food_tags_map m ON t.tag_id=m.tag_id WHERE m.food_id=$1")
			.bind(rec.food_id)
			.fetch_all(db)
			.await?;
        out_list.push(FoodOut::from((rec, tag_rows, vec![MarkTypeEnum::LIKE])));
    }
    Ok(HttpResponse::Ok().json(&out_list))
}

#[utoipa::path(
	post,
	path = "/foods/blind_box/draw",
	tag = "菜品",
	request_body = BlindBoxDrawInput,
	responses((status = 200, body = BlindBoxDrawResultOut)),
	security(("cookie_auth" = []))
)]
pub async fn draw_blind_box(
    token: UserToken,
    state: State<Arc<AppState>>,
    data: ntex::web::types::Json<BlindBoxDrawInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 获取 group_id，如果为空尝试用户所属第一个 group
    let group_id = if let Some(gid) = data.group_id {
        gid
    } else {
        sqlx::query_scalar("SELECT group_id FROM association_group_members WHERE user_id=$1 ORDER BY created_at LIMIT 1")
			.bind(token.user_id as i64)
			.fetch_optional(db)
			.await?
			.unwrap_or(0)
    };
    if group_id == 0 {
        return Err(CustomError::BadRequest("未找到绑定组".into()));
    }
    let limit_each = data.limit_each.unwrap_or(1) as i64;
    let mut results: Vec<BlindBoxFoodSnapshot> = Vec::new();
    for cat in &data.food_types {
        let rows = sqlx::query(
			"SELECT food_id, food_name, food_photo, food_types FROM foods WHERE group_id=$1 AND food_types=$2 AND food_status='NORMAL' AND apply_status='APPROVED' ORDER BY random() LIMIT $3"
		)
		.bind(group_id as i64)
		.bind(*cat as i16)
		.bind(limit_each)
		.fetch_all(db)
		.await?;
        for r in rows {
            results.push(BlindBoxFoodSnapshot {
                food_id: r.get::<i64, _>(0),
                food_name: r.get::<String, _>(1),
                food_photo: r.get::<Option<String>, _>(2),
                category: FoodCategory::from_i32(r.get::<i16, _>(3) as i32)
                    .unwrap_or(FoodCategory::Breakfast),
            });
        }
    }
    Ok(HttpResponse::Ok().json(&BlindBoxDrawResultOut {
        results,
        requested_types: data.food_types.clone(),
    }))
}
