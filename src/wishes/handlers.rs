use crate::{
    errors::CustomError,
    models::{
        pagination::{decode_cursor, encode_cursor, CursorPage},
        users::UserToken,
        wishes::{
            WishClaimStatusEnum, WishCreateInput, WishOut, WishQuery, WishRecord, WishStatusEnum,
            WishUpdateInput,
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

    // Require group_id or try to find one?
    // For now let's assume if not provided, we might return empty or error.
    // Better to require it or pick the user's primary group.
    // Let's assume the client sends it. If not, we return empty list for now or error.
    let Some(group_id) = query.group_id else {
        return Err(CustomError::BadRequest("Missing group_id".into()));
    };

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT w.wish_id, w.wish_name, w.wish_cost, w.status, w.created_by, w.group_id, w.created_at, w.updated_at, \
         wc.status as claim_status, wc.user_id as claimant_id, wc.id as claim_id \
         FROM wishes w \
         LEFT JOIN wish_claims wc ON w.wish_id = wc.wish_id AND wc.status != 'CANCELLED' \
         WHERE w.group_id = "
    );
    qb.push_bind(group_id);

    qb.push(" AND (w.created_by = ");
    qb.push_bind(user_token.user_id);
    qb.push(" OR (w.status = 'ON' AND wc.id IS NULL) OR wc.user_id = ");
    qb.push_bind(user_token.user_id);
    qb.push(") ");

    if let Some(cursor_str) = &query.cursor {
        if let Some(cursor) = decode_cursor::<WishCursor>(cursor_str) {
            qb.push(" AND (w.created_at, w.wish_id) < (");
            qb.push_bind(cursor.created_at);
            qb.push(", ");
            qb.push_bind(cursor.wish_id);
            qb.push(") ");
        }
    }

    qb.push(" ORDER BY w.created_at DESC, w.wish_id DESC ");
    qb.push(" LIMIT ");
    qb.push_bind(limit + 1);

    let rows = qb.build().fetch_all(&state.db_pool).await?;

    let has_more = rows.len() > limit as usize;
    let mut rows = rows;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&WishCursor {
                created_at: r.get("created_at"),
                wish_id: r.get("wish_id"),
            })
        })
    } else {
        None
    };

    let items: Vec<WishOut> = rows
        .into_iter()
        .map(|r| {
            let claim_status: Option<WishClaimStatusEnum> =
                r.try_get("claim_status").ok().flatten();
            let claim_id: Option<i64> = r.try_get("claim_id").ok().flatten();
            let claimant_id: Option<i64> = r.try_get("claimant_id").ok().flatten();
            WishOut {
                wish_id: r.get("wish_id"),
                wish_name: r.get("wish_name"),
                wish_cost: r.get("wish_cost"),
                status: r.get("status"),
                created_by: r.get("created_by"),
                group_id: r.get("group_id"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                claim_status,
                claimant_id,
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
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

    let db = &state.db_pool;
    let row = sqlx::query(
        "INSERT INTO wishes (wish_name, wish_cost, created_by, group_id) VALUES ($1,$2,$3,$4) RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, created_at, updated_at"
    )
    .bind(&data.wish_name)
    .bind(data.wish_cost)
    .bind(user_token.user_id)
    .bind(data.group_id)
    .fetch_one(db).await?;

    let rec = WishRecord {
        wish_id: row.get("wish_id"),
        wish_name: row.get("wish_name"),
        wish_cost: row.get("wish_cost"),
        status: WishStatusEnum::ON,
        created_by: row.get("created_by"),
        group_id: row.get("group_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    Ok(HttpResponse::Created().json(&WishOut::from(rec)))
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
    let row = sqlx::query(
        "SELECT w.wish_id, w.wish_name, w.wish_cost, w.status, w.created_by, w.group_id, w.created_at, w.updated_at, \
         wc.status as claim_status, wc.user_id as claimant_id, wc.id as claim_id \
         FROM wishes w \
         LEFT JOIN wish_claims wc ON w.wish_id = wc.wish_id AND wc.status != 'CANCELLED' \
         WHERE w.wish_id=$1"
    )
    .bind(*id)
    .fetch_optional(&state.db_pool).await?;

    let Some(r) = row else {
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };

    // Visibility check (optional, but good practice)
    // For now we allow if they know the ID, but maybe we should check group?
    // Let's assume if they have the ID and are in the group they can see it.

    let claim_status: Option<WishClaimStatusEnum> = r.try_get("claim_status").ok().flatten();
    let claimant_id: Option<i64> = r.try_get("claimant_id").ok().flatten();

    let out = WishOut {
        wish_id: r.get("wish_id"),
        wish_name: r.get("wish_name"),
        wish_cost: r.get("wish_cost"),
        status: r.get("status"),
        created_by: r.get("created_by"),
        group_id: r.get("group_id"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        claim_status: claim_status,
        claimant_id,
    };
    Ok(HttpResponse::Ok().json(&out))
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
        .push(" RETURNING wish_id, wish_name, wish_cost, status, created_by, group_id, created_at, updated_at");

    let updated = qb.build().fetch_one(db).await?;
    let rec = WishRecord {
        wish_id: updated.get("wish_id"),
        wish_name: updated.get("wish_name"),
        wish_cost: updated.get("wish_cost"),
        status: updated.get("status"),
        created_by,
        group_id: updated.get("group_id"),
        created_at: updated.get("created_at"),
        updated_at: updated.get("updated_at"),
    };
    Ok(HttpResponse::Ok().json(&WishOut::from(rec)))
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
        "SELECT wish_id, wish_name, wish_cost, status, created_by, group_id, created_at, updated_at FROM wishes WHERE wish_id=$1"
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

    // Soft delete (OFF)
    sqlx::query("UPDATE wishes SET status='OFF', updated_at=NOW() WHERE wish_id=$1")
        .bind(*id)
        .execute(db)
        .await?;

    let rec = WishRecord {
        wish_id: r.get("wish_id"),
        wish_name: r.get("wish_name"),
        wish_cost: r.get("wish_cost"),
        status: WishStatusEnum::OFF,
        created_by,
        group_id: r.get("group_id"),
        created_at: r.get("created_at"),
        updated_at: chrono::Utc::now(),
    };
    Ok(HttpResponse::Ok().json(&WishOut::from(rec)))
}
