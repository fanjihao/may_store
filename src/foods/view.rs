use crate::{
    errors::CustomError,
    models::foods::{
        BlindBoxDrawInput, BlindBoxDrawResultOut, BlindBoxFoodSnapshot, FoodCategory,
        FoodFilterQuery, FoodOut, FoodTagOut, FoodWithStatsRecord, MarkTypeEnum, TagRecord,
    },
    models::users::UserToken,
    AppState,
};
use ntex::web::{
    types::{Path, Query, State},
    HttpResponse, Responder,
};
use sqlx::{QueryBuilder, Row};
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
	responses((status = 200, body = Vec<FoodOut>)),
    security(("cookie_auth"=[]))
)]
pub async fn get_foods(
    state: State<Arc<AppState>>,
    token: UserToken,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT f.food_id, f.food_name, f.food_photo, f.food_types, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id WHERE f.is_del=0"
    );
    if let Some(kw) = &q.keyword {
        qb.push(" AND f.food_name ILIKE '%' || ")
            .push_bind(kw)
            .push(" || '%'");
    }
    if let Some(cat) = q.category {
        qb.push(" AND f.food_types = ").push_bind(cat as i16);
    }
    if let Some(fs) = q.food_status {
        qb.push(" AND f.food_status = ").push_bind(fs);
    }
    if let Some(as_) = q.apply_status {
        qb.push(" AND f.apply_status = ").push_bind(as_);
    }
    if let Some(sr) = q.submit_role {
        qb.push(" AND f.submit_role = ").push_bind(sr);
    }
    if let Some(gid) = q.group_id {
        qb.push(" AND f.group_id = ").push_bind(gid);
    }
    if let Some(user) = q.created_by {
        qb.push(" AND f.created_by = ").push_bind(user);
    }
    if let Some(tag_ids) = &q.tag_ids {
        if !tag_ids.is_empty() {
            qb.push(" AND EXISTS (SELECT 1 FROM food_tags_map m WHERE m.food_id=f.food_id AND m.tag_id = ANY(").push_bind(tag_ids).push("))");
        }
    }
    if q.only_active.unwrap_or(false) {
        qb.push(" AND f.food_status='NORMAL' AND f.apply_status='APPROVED'");
    }
    qb.push(" ORDER BY f.created_at DESC LIMIT 100");
    let rows: Vec<FoodWithStatsRecord> = qb.build_query_as().fetch_all(db).await?;

    // ===== 批量标签查询 =====
    let food_ids: Vec<i64> = rows.iter().map(|r| r.food_id).collect();
    let mut tags_map: std::collections::HashMap<i64, Vec<TagRecord>> = std::collections::HashMap::new();
    if !food_ids.is_empty() {
        let tag_rows = sqlx::query(
            "SELECT m.food_id, t.tag_id, t.tag_name, t.sort, t.created_at FROM food_tags_map m JOIN tags t ON t.tag_id=m.tag_id WHERE m.food_id = ANY($1)"
        )
        .bind(&food_ids)
        .fetch_all(db)
        .await?;
        for r in tag_rows {
            let fid: i64 = r.get("food_id");
            let tag = TagRecord {
                tag_id: r.get("tag_id"),
                tag_name: r.get("tag_name"),
                sort: r.get("sort"),
                created_at: r.get("created_at"),
            };
            tags_map.entry(fid).or_default().push(tag);
        }
    }

    // ===== 批量用户标记查询 =====
    let mut marks_map: std::collections::HashMap<i64, Vec<MarkTypeEnum>> = std::collections::HashMap::new();
    if !food_ids.is_empty() {
        let mark_rows = sqlx::query(
            "SELECT food_id, mark_type::text AS mark_type FROM user_food_mark WHERE user_id=$1 AND food_id = ANY($2)"
        )
        .bind(token.user_id as i64)
        .bind(&food_ids)
        .fetch_all(db)
        .await?;
        for r in mark_rows {
            let fid: i64 = r.get("food_id");
            let mtxt: String = r.get("mark_type");
            let enum_val = match mtxt.as_str() {
                "LIKE" => Some(MarkTypeEnum::LIKE),
                "NOT_RECOMMEND" => Some(MarkTypeEnum::NOT_RECOMMEND),
                _ => None,
            };
            if let Some(ev) = enum_val { marks_map.entry(fid).or_default().push(ev); }
        }
    }

    // ===== 组装输出 =====
    let mut out_list: Vec<FoodOut> = Vec::with_capacity(rows.len());
    for rec in rows {
        let tag_vec = tags_map.remove(&rec.food_id).unwrap_or_default();
        let mark_vec = marks_map.remove(&rec.food_id).unwrap_or_default();
        out_list.push(FoodOut::from_with_stats(rec, tag_vec, mark_vec));
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
    let rec_opt = sqlx::query_as::<_, FoodWithStatsRecord>(
        "SELECT f.food_id, f.food_name, f.food_photo, f.food_types, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id WHERE f.food_id=$1"
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
    Ok(HttpResponse::Ok().json(&FoodOut::from_with_stats(rec, tag_rows, mark_enums)))
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
    let rows: Vec<FoodWithStatsRecord> = sqlx::query_as(
        "SELECT f.food_id, f.food_name, f.food_photo, f.food_types, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time \
         FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id JOIN user_food_mark m ON f.food_id=m.food_id WHERE m.user_id=$1 AND m.mark_type='LIKE'"
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
        out_list.push(FoodOut::from_with_stats(
            rec,
            tag_rows,
            vec![MarkTypeEnum::LIKE],
        ));
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
