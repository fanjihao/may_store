use crate::{
    errors::CustomError,
    models::{
        foods::{
            BlindBoxDrawInput, BlindBoxDrawResultOut, BlindBoxFoodSnapshot, FoodFilterQuery,
            FoodOut, FoodTagOut, FoodWithStatsRecord, MarkTypeEnum, TagRecord,
        },
        pagination::{decode_cursor, encode_cursor, CursorPage},
        users::UserToken,
    },
    AppState,
};
use ntex::web::{
    types::{Path, Query, State},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row};
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize)]
pub struct FoodCursor {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub food_id: i64,
}

#[utoipa::path(
	get,
	path = "/foods",
	tag = "菜品",
	params(FoodFilterQuery),
	responses((status = 200, body = CursorPage<FoodOut>)),
    security(("cookie_auth"=[]))
)]
pub async fn get_foods(
    state: State<Arc<AppState>>,
    token: UserToken,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, f.tag_id, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id WHERE f.is_del=0"
    );
    if let Some(kw) = &q.keyword {
        qb.push(" AND f.food_name ILIKE '%' || ")
            .push_bind(kw)
            .push(" || '%'");
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
    } else if let Some(user) = q.created_by {
        qb.push(" AND f.created_by = ").push_bind(user);
    }
    if let Some(tag_id) = q.tag_id {
        qb.push(" AND f.tag_id = ").push_bind(tag_id);
    }
    if q.only_active.unwrap_or(false) {
        qb.push(" AND f.food_status='NORMAL' AND f.apply_status='APPROVED'");
    }

    if let Some(cursor_str) = &q.cursor {
        if let Some(cursor) = decode_cursor::<FoodCursor>(cursor_str) {
            qb.push(" AND (f.created_at, f.food_id) < (");
            qb.push_bind(cursor.created_at);
            qb.push(", ");
            qb.push_bind(cursor.food_id);
            qb.push(")");
        }
    }

    qb.push(" ORDER BY f.created_at DESC, f.food_id DESC LIMIT ")
        .push_bind(limit + 1);

    let mut rows: Vec<FoodWithStatsRecord> = qb.build_query_as().fetch_all(db).await?;

    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&FoodCursor {
                created_at: r.created_at,
                food_id: r.food_id,
            })
        })
    } else {
        None
    };

    // ===== 批量标签查询 =====
    let tag_ids: Vec<i64> = rows.iter().filter_map(|r| r.tag_id).collect();
    let mut tags_map: std::collections::HashMap<i64, TagRecord> = std::collections::HashMap::new();
    if !tag_ids.is_empty() {
        let tag_rows = sqlx::query_as::<_, TagRecord>("SELECT * FROM tags WHERE tag_id = ANY($1)")
            .bind(&tag_ids)
            .fetch_all(db)
            .await?;
        for t in tag_rows {
            tags_map.insert(t.tag_id, t);
        }
    }

    // ===== 批量用户标记查询 =====
    let mut marks_map: std::collections::HashMap<i64, Vec<MarkTypeEnum>> =
        std::collections::HashMap::new();
    if !rows.is_empty() {
        let food_ids: Vec<i64> = rows.iter().map(|r| r.food_id).collect();
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
                "NOT_RECOMMEND" => Some(MarkTypeEnum::NotRecommend),
                _ => None,
            };
            if let Some(ev) = enum_val {
                marks_map.entry(fid).or_default().push(ev);
            }
        }
    }

    // ===== 组装输出 =====
    let mut items: Vec<FoodOut> = Vec::with_capacity(rows.len());
    for rec in rows {
        let tag = rec.tag_id.and_then(|tid| tags_map.get(&tid).cloned());
        let mark_vec = marks_map.remove(&rec.food_id).unwrap_or_default();
        items.push(FoodOut::from_with_stats(rec, tag, mark_vec));
    }

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
    }))
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
        "SELECT f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, f.tag_id, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id WHERE f.food_id=$1"
    )
	.bind(id.0)
	.fetch_optional(db)
	.await?;
    let rec = match rec_opt {
        Some(r) => r,
        None => return Err(CustomError::BadRequest("未找到菜品".into())),
    };
    let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
        sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
            .bind(tid)
            .fetch_optional(db)
            .await?
    } else {
        None
    };
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
            "NOT_RECOMMEND" => Some(MarkTypeEnum::NotRecommend),
            _ => None,
        })
        .collect();
    Ok(HttpResponse::Ok().json(&FoodOut::from_with_stats(rec, tag_row, mark_enums)))
}

#[utoipa::path(
	get,
	path = "/food_tags",
	tag = "标签",
    params(FoodFilterQuery),
	responses((status = 200, body = Vec<FoodTagOut>)),
    security(("cookie_auth" = []))
)]
pub async fn get_tags(
    state: State<Arc<AppState>>,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let group_id = match q.group_id {
        Some(id) => id,
        None => return Err(CustomError::BadRequest("缺少group_id参数".into())),
    };

    let rows: Vec<TagRecord> = sqlx::query_as(
        "SELECT tag_id, tag_name, icon, group_id, sort, created_at FROM tags WHERE group_id=$1 ORDER BY sort NULLS LAST, tag_id"
    )
    .bind(group_id)
    .fetch_all(db)
    .await?;

    let tag_ids: Vec<i64> = rows.iter().map(|r| r.tag_id).collect();
    let mut counts_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if !tag_ids.is_empty() {
        let count_rows = sqlx::query(
            "SELECT tag_id, COUNT(*) as cnt FROM foods WHERE tag_id = ANY($1) AND is_del=0 GROUP BY tag_id"
        )
        .bind(&tag_ids)
        .fetch_all(db)
        .await?;

        for r in count_rows {
            let tid: i64 = r.get("tag_id");
            let cnt: i64 = r.get("cnt");
            counts_map.insert(tid, cnt);
        }
    }

    Ok(HttpResponse::Ok().json(
        &rows
            .into_iter()
            .map(|r| {
                let cnt = counts_map.get(&r.tag_id).copied().unwrap_or(0);
                FoodTagOut {
                    tag_id: r.tag_id,
                    tag_name: r.tag_name,
                    icon: r.icon,
                    sort: r.sort,
                    food_count: Some(cnt),
                }
            })
            .collect::<Vec<_>>(),
    ))
}

#[utoipa::path(
	get,
	path = "/foods/marks",
	tag = "菜品",
	params(FoodFilterQuery),
	responses((status = 200, body = CursorPage<FoodOut>)),
	security(("cookie_auth" = []))
)]
pub async fn get_marked_foods(
    token: UserToken,
    state: State<Arc<AppState>>,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);

    let mut qb = QueryBuilder::<sqlx::Postgres>::new(
        "SELECT f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, f.tag_id, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time \
         FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id JOIN user_food_mark m ON f.food_id=m.food_id WHERE m.user_id="
    );
    qb.push_bind(token.user_id as i64);
    qb.push(" AND m.mark_type='LIKE'");

    if let Some(cursor_str) = &q.cursor {
        if let Some(cursor) = decode_cursor::<FoodCursor>(cursor_str) {
            qb.push(" AND (f.created_at, f.food_id) < (");
            qb.push_bind(cursor.created_at);
            qb.push(", ");
            qb.push_bind(cursor.food_id);
            qb.push(")");
        }
    }

    qb.push(" ORDER BY f.created_at DESC, f.food_id DESC LIMIT ")
        .push_bind(limit + 1);

    let mut rows: Vec<FoodWithStatsRecord> = qb.build_query_as().fetch_all(db).await?;

    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&FoodCursor {
                created_at: r.created_at,
                food_id: r.food_id,
            })
        })
    } else {
        None
    };

    let mut items = Vec::new();
    for rec in rows {
        let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
            sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
                .bind(tid)
                .fetch_optional(db)
                .await?
        } else {
            None
        };
        items.push(FoodOut::from_with_stats(
            rec,
            tag_row,
            vec![MarkTypeEnum::LIKE],
        ));
    }

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
    }))
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
    for tag_id in &data.tag_ids {
        let rows = sqlx::query(
			"SELECT food_id, food_name, food_photo FROM foods WHERE group_id=$1 AND tag_id=$2 AND food_status='NORMAL' AND apply_status='APPROVED' ORDER BY random() LIMIT $3"
		)
		.bind(group_id as i64)
		.bind(*tag_id)
		.bind(limit_each)
		.fetch_all(db)
		.await?;
        for r in rows {
            results.push(BlindBoxFoodSnapshot {
                food_id: r.get::<i64, _>(0),
                food_name: r.get::<String, _>(1),
                food_photo: r.get::<Option<String>, _>(2),
            });
        }
    }
    Ok(HttpResponse::Ok().json(&BlindBoxDrawResultOut {
        results,
        requested_tags: data.tag_ids.clone(),
    }))
}
