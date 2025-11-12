use crate::{
    errors::CustomError,
    models::{
        users::UserToken,
        wishes::{
            WishClaimCheckinCreateInput, WishClaimCheckinOut, WishClaimCheckinRecord,
            WishClaimStatusEnum,
        },
    },
    AppState,
};
use chrono::Utc;
use ntex::web::{
    types::{Json, Path, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(post, path="/wish_claims/{claim_id}/checkins", tag="心愿", params(("claim_id"=i64, Path, description="兑换记录ID")), request_body=WishClaimCheckinCreateInput, responses((status=201, body=WishClaimCheckinOut)))]
pub async fn create_wish_claim_checkin(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    claim_id: Path<i64>,
    body: Json<WishClaimCheckinCreateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 校验兑换记录存在且属于当前用户且已完成
    let claim_row = sqlx::query("SELECT id, user_id, status FROM wish_claims WHERE id=$1")
        .bind(*claim_id)
        .fetch_optional(db)
        .await?;
    let Some(cr) = claim_row else {
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };
    let c_user: i64 = cr.get("user_id");
    if c_user != user_token.user_id {
        return Err(CustomError::BadRequest("只能为自己的兑换打卡".into()));
    }
    let status_str: String = cr.get("status");
    if status_str != "DONE" {
        return Err(CustomError::BadRequest("仅已完成的兑换可打卡".into()));
    }
    let checkin_time = body.checkin_time.unwrap_or_else(Utc::now);
    let row = sqlx::query("INSERT INTO wish_claim_checkins (claim_id, user_id, photo_url, location_text, mood_text, feeling_text, checkin_time) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id, claim_id, user_id, photo_url, location_text, mood_text, feeling_text, checkin_time, created_at")
        .bind(*claim_id)
        .bind(user_token.user_id)
        .bind(&body.photo_url)
        .bind(&body.location_text)
        .bind(&body.mood_text)
        .bind(&body.feeling_text)
        .bind(checkin_time)
        .fetch_one(db).await?;
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

#[utoipa::path(get, path="/wish_claims/{claim_id}/checkins", tag="心愿", params(("claim_id"=i64, Path, description="兑换记录ID")), responses((status=200, body=[WishClaimCheckinOut])))]
pub async fn list_wish_claim_checkins(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    claim_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 权限：只能查看自己的兑换的打卡
    let own = sqlx::query("SELECT user_id FROM wish_claims WHERE id=$1")
        .bind(*claim_id)
        .fetch_optional(db)
        .await?;
    let Some(r) = own else {
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };
    let u: i64 = r.get("user_id");
    if u != user_token.user_id {
        return Err(CustomError::BadRequest("只能查看自己兑换的打卡".into()));
    }
    let rows = sqlx::query("SELECT id, claim_id, user_id, photo_url, location_text, mood_text, feeling_text, checkin_time, created_at FROM wish_claim_checkins WHERE claim_id=$1 ORDER BY checkin_time DESC")
        .bind(*claim_id).fetch_all(db).await?;
    let list: Vec<WishClaimCheckinOut> = rows
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
    Ok(HttpResponse::Ok().json(&list))
}
