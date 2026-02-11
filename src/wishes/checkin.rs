use crate::{
    errors::CustomError,
    models::{
        users::UserToken,
        wishes::{
            WishClaimCheckinCreateInput,
            WishClaimCheckinUpdateInput,
            WishClaimCheckinOut,
            WishClaimCheckinQuery,
            WishClaimCheckinRecord,
            WishClaimStatusEnum,
        },
        pagination::{decode_cursor, encode_cursor, CursorPage},
    },
    AppState,
};
use chrono::Utc;
use ntex::web::{ types::{ Json, Path, State, Query }, HttpResponse, Responder };
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row};
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize)]
pub struct CheckinCursor {
    pub checkin_time: chrono::DateTime<chrono::Utc>,
    pub id: i64,
}

#[utoipa::path(
    post,
    path = "/wish_claims/{claim_id}/checkins",
    tag = "签到",
    params(("claim_id" = i64, Path, description = "兑换记录ID")),
    request_body = WishClaimCheckinCreateInput,
    responses((status = 201, body = WishClaimCheckinOut))
)]
pub async fn create_wish_claim_checkin(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    claim_id: Path<i64>,
    body: Json<WishClaimCheckinCreateInput>
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;
    // 校验兑换记录存在且属于当前用户
    let claim_row = sqlx
        ::query("SELECT id, user_id, status FROM wish_claims WHERE id=$1 FOR UPDATE")
        .bind(*claim_id)
        .fetch_optional(&mut *tx).await?;
    let Some(cr) = claim_row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };
    let c_user: i64 = cr.get("user_id");
    if c_user != user_token.user_id {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("只能为自己的兑换打卡".into()));
    }

    let status: WishClaimStatusEnum = cr.get("status");
    if status == WishClaimStatusEnum::CANCELLED {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("已取消的兑换无法打卡".into()));
    }

    let checkin_time = body.checkin_time.unwrap_or_else(Utc::now);
    let row = sqlx
        ::query(
            "INSERT INTO wish_claim_checkins (claim_id, user_id, photo_url, location_text, mood_text, feeling_text, checkin_time) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id, claim_id, user_id, photo_url, location_text, mood_text, feeling_text, checkin_time, created_at"
        )
        .bind(*claim_id)
        .bind(user_token.user_id)
        .bind(&body.photo_url)
        .bind(&body.location_text)
        .bind(&body.mood_text)
        .bind(&body.feeling_text)
        .bind(checkin_time)
        .fetch_one(&mut *tx).await?;

    // 自动更新状态为 DONE
    if status != WishClaimStatusEnum::DONE {
        sqlx::query("UPDATE wish_claims SET status=$1, fulfill_at=NOW(), updated_at=NOW() WHERE id=$2")
            .bind(WishClaimStatusEnum::DONE)
            .bind(*claim_id)
            .execute(&mut *tx).await?;
    }

    tx.commit().await?;

    let rec = WishClaimCheckinRecord {
        id: row.get("id"),
        claim_id: row.get("claim_id"),
        user_id: row.get("user_id"),
        photo_url: row.try_get("photo_url").ok(),
        location_text: row.try_get("location_text").ok(),
        mood_text: row.try_get("mood_text").ok(),
        feeling_text: row.try_get("feeling_text").ok(),
        checkin_time: row.get("checkin_time"),
        created_at: row.get("created_at"),
    };
    Ok(HttpResponse::Created().json(&WishClaimCheckinOut::from(rec)))
}

#[utoipa::path(
    get,
    path = "/wish_claims/{claim_id}/checkins",
    tag = "签到",
    params(("claim_id" = i64, Path, description = "兑换记录ID"), WishClaimCheckinQuery),
    responses((status = 200, body = CursorPage<WishClaimCheckinOut>))
)]
pub async fn list_wish_claim_checkins(
    _: UserToken,
    state: State<Arc<AppState>>,
    claim_id: Path<i64>,
    query: Query<WishClaimCheckinQuery>
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 权限：只能查看自己的兑换的打卡
    let own = sqlx
        ::query("SELECT user_id FROM wish_claims WHERE id=$1")
        .bind(*claim_id)
        .fetch_optional(db).await?;
    let Some(_) = own else {
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };

    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let mut qb = QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, claim_id, user_id, photo_url, location_text, mood_text, feeling_text, checkin_time, created_at FROM wish_claim_checkins WHERE claim_id="
    );
    qb.push_bind(*claim_id);

    if let Some(cursor_str) = &query.cursor {
        if let Some(cursor) = decode_cursor::<CheckinCursor>(cursor_str) {
            qb.push(" AND (checkin_time, id) < (");
            qb.push_bind(cursor.checkin_time);
            qb.push(", ");
            qb.push_bind(cursor.id);
            qb.push(") ");
        }
    }

    qb.push(" ORDER BY checkin_time DESC, id DESC LIMIT ");
    qb.push_bind(limit + 1);

    let mut rows = qb.build().fetch_all(db).await?;

    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&CheckinCursor {
                checkin_time: r.get("checkin_time"),
                id: r.get("id"),
            })
        })
    } else {
        None
    };

    let items: Vec<WishClaimCheckinOut> = rows
        .into_iter()
        .map(|row| WishClaimCheckinOut {
            id: row.get("id"),
            claim_id: row.get("claim_id"),
            user_id: row.get("user_id"),
            photo_url: row.try_get("photo_url").ok(),
            location_text: row.try_get("location_text").ok(),
            mood_text: row.try_get("mood_text").ok(),
            feeling_text: row.try_get("feeling_text").ok(),
            checkin_time: row.get("checkin_time"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
    }))
}

#[utoipa::path(
    put,
    path = "/wish_claims/checkins/{id}",
    tag = "签到",
    params(("id" = i64, Path, description = "打卡记录ID")),
    request_body = WishClaimCheckinUpdateInput,
    responses((status = 200, body = WishClaimCheckinOut))
)]
pub async fn update_wish_claim_checkin(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishClaimCheckinUpdateInput>
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    // 校验记录存在且属于当前用户
    let checkin = sqlx::query_as::<_, WishClaimCheckinRecord>(
        "SELECT * FROM wish_claim_checkins WHERE id = $1"
    )
    .bind(*id)
    .fetch_optional(db)
    .await?;

    let Some(c) = checkin else {
        return Err(CustomError::BadRequest("打卡记录不存在".into()));
    };

    if c.user_id != user_token.user_id {
        return Err(CustomError::BadRequest("无权修改他人的打卡记录".into()));
    }

    let photo_url = body.photo_url.as_ref().or(c.photo_url.as_ref());
    let location_text = body.location_text.as_ref().or(c.location_text.as_ref());
    let mood_text = body.mood_text.as_ref().or(c.mood_text.as_ref());
    let feeling_text = body.feeling_text.as_ref().or(c.feeling_text.as_ref());
    let checkin_time = body.checkin_time.unwrap_or(c.checkin_time);

    let row = sqlx::query(
        "UPDATE wish_claim_checkins SET photo_url=$1, location_text=$2, mood_text=$3, feeling_text=$4, checkin_time=$5 WHERE id=$6 RETURNING *"
    )
    .bind(photo_url)
    .bind(location_text)
    .bind(mood_text)
    .bind(feeling_text)
    .bind(checkin_time)
    .bind(*id)
    .fetch_one(db)
    .await?;

    let rec = WishClaimCheckinRecord {
        id: row.get("id"),
        claim_id: row.get("claim_id"),
        user_id: row.get("user_id"),
        photo_url: row.try_get("photo_url").ok(),
        location_text: row.try_get("location_text").ok(),
        mood_text: row.try_get("mood_text").ok(),
        feeling_text: row.try_get("feeling_text").ok(),
        checkin_time: row.get("checkin_time"),
        created_at: row.get("created_at"),
    };

    Ok(HttpResponse::Ok().json(&WishClaimCheckinOut::from(rec)))
}
