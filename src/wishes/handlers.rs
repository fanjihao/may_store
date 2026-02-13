use crate::{
    errors::CustomError,
    models::{
        pagination::{decode_cursor, encode_cursor, CursorPage},
        users::UserToken,
        wishes::{
            WishCreateInput, WishFeedbackRecord, WishOut, WishQuery, WishRecord, WishUpdateInput,
        },
    },
    AppState,
};
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row};
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize)]
pub struct WishCursor {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub wish_id: i64,
}

// 1. Get wish list
#[utoipa::path(
    get,
    path = "/wishes",
    tag = "心愿",
    params(WishQuery),
    responses((status = 200, body = CursorPage<WishOut>))
)]
pub async fn list_wishes(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<WishQuery>,
) -> Result<impl Responder, CustomError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    // Require group_id or assume current user's group context?
    // Client should provide group_id.
    let Some(group_id) = query.group_id else {
        return Err(CustomError::BadRequest("Missing group_id".into()));
    };

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT * FROM wishes WHERE group_id = "
    );
    qb.push_bind(group_id);

    // Visibility Logic:
    // 1. Created by me
    // 2. OR Claimed by me
    // 3. OR Status != CLOSED (Visible to group)
    let visibility_sql = " AND (created_by = $2 OR claimed_by = $2 OR status != 'CLOSED') ";

    // 1. Get total count (before cursor)
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM wishes WHERE group_id = $1 {}",
        visibility_sql
    ))
    .bind(group_id)
    .bind(user_token.user_id)
    .fetch_one(&state.db_pool)
    .await?;

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT * FROM wishes WHERE group_id = "
    );
    qb.push_bind(group_id);
    qb.push(" AND (created_by = ");
    qb.push_bind(user_token.user_id);
    qb.push(" OR claimed_by = ");
    qb.push_bind(user_token.user_id);
    qb.push(" OR status != 'CLOSED') ");

    if let Some(cursor_str) = &query.cursor {
        if let Some(cursor) = decode_cursor::<WishCursor>(cursor_str) {
            qb.push(" AND (created_at, wish_id) < (");
            qb.push_bind(cursor.created_at);
            qb.push(", ");
            qb.push_bind(cursor.wish_id);
            qb.push(") ");
        }
    }

    qb.push(" ORDER BY created_at DESC, wish_id DESC ");
    qb.push(" LIMIT ");
    qb.push_bind((limit + 1) as i64);

    let rows = qb.build_query_as::<WishRecord>()
        .fetch_all(&state.db_pool)
        .await?;

    let has_more = rows.len() > limit as usize;
    let mut rows = rows;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&WishCursor {
                created_at: r.created_at,
                wish_id: r.wish_id,
            })
        })
    } else {
        None
    };

    // For list view, we skip fetching feedback details to be lightweight,
    // or maybe fetch them if needed? Let's keep it lightweight: feedback=None.
    let items: Vec<WishOut> = rows
        .into_iter()
        .map(|r| WishOut::from_record(r, None))
        .collect();

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
        total: Some(total),
    }))
}

// 2. Create wish
#[utoipa::path(
    post,
    path = "/wishes",
    tag = "心愿",
    request_body = WishCreateInput,
    responses((status = 201, body = WishOut))
)]
pub async fn create_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<WishCreateInput>,
) -> Result<impl Responder, CustomError> {
    if data.wish_name.trim().is_empty() {
        return Err(CustomError::BadRequest("心愿名称不能为空".into()));
    }
    if data.wish_cost <= 0 {
        return Err(CustomError::BadRequest("心愿积分必须大于0".into()));
    }

    // Verify user is in group
    let is_member =
        sqlx::query("SELECT 1 FROM association_group_members WHERE user_id=$1 AND group_id=$2")
            .bind(user_token.user_id)
            .bind(data.group_id)
            .fetch_optional(&state.db_pool)
            .await?;

    if is_member.is_none() {
        return Err(CustomError::BadRequest("您不是该组成员".into()));
    }

    let rec = sqlx::query_as::<_, WishRecord>(
        "INSERT INTO wishes (wish_name, wish_cost, created_by, group_id, status) VALUES ($1,$2,$3,$4,'CREATED') RETURNING *"
    )
    .bind(&data.wish_name)
    .bind(data.wish_cost)
    .bind(user_token.user_id)
    .bind(data.group_id)
    .fetch_one(&state.db_pool).await?;

    Ok(HttpResponse::Created().json(&WishOut::from_record(rec, None)))
}

// Get single wish
#[utoipa::path(
    get,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn get_wish(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let rec = sqlx::query_as::<_, WishRecord>("SELECT * FROM wishes WHERE wish_id=$1")
        .bind(*id)
        .fetch_optional(&state.db_pool)
        .await?;

    let Some(r) = rec else {
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };

    // Fetch feedback if exists
    let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
        "SELECT * FROM wish_feedbacks WHERE wish_id=$1"
    )
    .bind(*id)
    .fetch_optional(&state.db_pool)
    .await?;

    Ok(HttpResponse::Ok().json(&WishOut::from_record(r, feedback)))
}

// 3. Edit wish
#[utoipa::path(
    put,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishUpdateInput,
    responses((status = 200, body = WishOut))
)]
pub async fn update_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<WishUpdateInput>,
) -> Result<impl Responder, CustomError> {
    if data.wish_name.is_none() && data.wish_cost.is_none() && data.status.is_none() {
        return Err(CustomError::BadRequest("无修改内容".into()));
    }
    let db = &state.db_pool;

    // Check ownership
    let row = sqlx::query("SELECT created_by FROM wishes WHERE wish_id=$1")
        .bind(*id)
        .fetch_optional(db)
        .await?;

    let Some(r) = row else {
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    let created_by: i64 = r.get("created_by");
    if created_by != user_token.user_id {
        return Err(CustomError::BadRequest("只能修改自己创建的心愿".into()));
    }

    // Build dynamic update
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("UPDATE wishes SET ");
    let mut first = true;
    if let Some(name) = &data.wish_name {
        if !first {
            qb.push(", ");
        }
        first = false;
        qb.push(" wish_name = ").push_bind(name);
    }
    if let Some(cost) = data.wish_cost {
        if !first {
            qb.push(", ");
        }
        first = false;
        qb.push(" wish_cost = ").push_bind(cost);
    }
    if let Some(st) = data.status {
        if !first {
            qb.push(", ");
        }
        qb.push(" status = ").push_bind(st);
    }

    qb.push(", updated_at = NOW() WHERE wish_id = ")
        .push_bind(*id)
        .push(" RETURNING *");

    let updated = qb.build_query_as::<WishRecord>().fetch_one(db).await?;

    // Fetch feedback just in case (though update doesn't touch feedback)
    let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
        "SELECT * FROM wish_feedbacks WHERE wish_id=$1"
    )
    .bind(*id)
    .fetch_optional(db)
    .await?;

    Ok(HttpResponse::Ok().json(&WishOut::from_record(updated, feedback)))
}

// 4. Delete wish
#[utoipa::path(
    delete,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn delete_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let row = sqlx::query(
        "SELECT created_by FROM wishes WHERE wish_id=$1"
    )
    .bind(*id)
    .fetch_optional(db).await?;

    let Some(r) = row else {
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    let created_by: i64 = r.get("created_by");
    if created_by != user_token.user_id {
        return Err(CustomError::BadRequest("只能删除自己创建的心愿".into()));
    }

    // Soft delete (CLOSED)
    let rec = sqlx::query_as::<_, WishRecord>(
        "UPDATE wishes SET status='CLOSED', updated_at=NOW() WHERE wish_id=$1 RETURNING *"
    )
    .bind(*id)
    .fetch_one(db)
    .await?;

    Ok(HttpResponse::Ok().json(&WishOut::from_record(rec, None)))
}
