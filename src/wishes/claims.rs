use crate::{
    errors::CustomError,
    models::{
        users::UserToken,
        wishes::{
            WishFeedbackInput, WishFeedbackRecord, WishOut, WishRecord, WishStatusEnum,
        },
    },
    AppState,
};
// use chrono::Utc;
use ntex::web::{
    types::{Json, Path, State},
    HttpResponse, Responder,
};
use sqlx::{types::Json as SqlxJson, Row, FromRow};
use std::sync::Arc;
// use utoipa::ToSchema;

// 5. Redeem wish
#[utoipa::path(
    post,
    path = "/wishes/{id}/redeem",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn redeem_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // Get wish with lock
    let wish_row = sqlx::query(
        "SELECT * FROM wishes WHERE wish_id=$1 FOR UPDATE"
    )
    .bind(*id)
    .fetch_optional(&mut *tx).await?;

    let Some(r) = wish_row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    // Map to WishRecord manually or via from_row if we use query_as earlier
    // But r is PgRow. Let's cast.
    let wish_rec = WishRecord::from_row(&r).map_err(|e| CustomError::InternalServerError(e.to_string()))?;

    if wish_rec.status != WishStatusEnum::CREATED {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("心愿状态不可兑换".into()));
    }

    if wish_rec.created_by == user_token.user_id {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("无法兑换自己的心愿".into()));
    }

    let cost = wish_rec.wish_cost;

    // Check points
    let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
        .bind(user_token.user_id)
        .fetch_one(&mut *tx).await?;
    let love_point: i32 = user_row.get("love_point");

    if love_point < cost {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("积分不足".into()));
    }

    let balance_after = love_point - cost;

    // Deduct points
    sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
        .bind(user_token.user_id)
        .bind(balance_after)
        .execute(&mut *tx).await?;

    // Transaction log
    sqlx::query(
        "INSERT INTO point_transactions(user_id, amount, type, ref_type, ref_id, balance_after) VALUES($1,$2,'WISH_COST',2,$3,$4)"
    )
    .bind(user_token.user_id)
    .bind(-cost)
    .bind(*id)
    .bind(balance_after)
    .execute(&mut *tx).await?;

    // Update wish
    let updated_rec = sqlx::query_as::<_, WishRecord>(
        "UPDATE wishes SET status='CLAIMED', claimed_by=$1, claimed_at=NOW(), claim_cost=$2, updated_at=NOW() WHERE wish_id=$3 RETURNING *"
    )
    .bind(user_token.user_id)
    .bind(cost)
    .bind(*id)
    .fetch_one(&mut *tx).await?;

    tx.commit().await?;

    Ok(HttpResponse::Ok().json(&WishOut::from_record(updated_rec, None)))
}

// 7. Submit/Update Feedback
#[utoipa::path(
    put,
    path = "/wishes/{id}/feedback",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishFeedbackInput,
    responses((status = 200, body = WishOut))
)]
pub async fn submit_feedback(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<WishFeedbackInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // Lock wish
    let wish_row = sqlx::query("SELECT * FROM wishes WHERE wish_id=$1 FOR UPDATE")
        .bind(*id)
        .fetch_optional(&mut *tx).await?;

    let Some(r) = wish_row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    let wish_rec = WishRecord::from_row(&r).map_err(|e| CustomError::InternalServerError(e.to_string()))?;

    // Check status
    // Must be CLAIMED or FINISHED (allow editing)
    if wish_rec.status != WishStatusEnum::CLAIMED && wish_rec.status != WishStatusEnum::FINISHED {
         tx.rollback().await.ok();
         return Err(CustomError::BadRequest("心愿状态不可反馈".into()));
    }

    // Check ownership: Must be the Claimer
    if wish_rec.claimed_by != Some(user_token.user_id) {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("只能为自己兑换的心愿提交反馈".into()));
    }

    let images_json = data.images.clone().map(SqlxJson);

    // Upsert feedback
    // Since we have UNIQUE(wish_id) on wish_feedbacks, we can use ON CONFLICT
    let feedback_rec = sqlx::query_as::<_, WishFeedbackRecord>(
        r#"
        INSERT INTO wish_feedbacks (wish_id, user_id, content, images, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        ON CONFLICT (wish_id)
        DO UPDATE SET content = EXCLUDED.content, images = EXCLUDED.images, updated_at = NOW()
        RETURNING *
        "#
    )
    .bind(*id)
    .bind(user_token.user_id)
    .bind(&data.content)
    .bind(images_json)
    .fetch_one(&mut *tx).await?;

    // Update wish status to FINISHED if it was CLAIMED
    let final_wish_rec = if wish_rec.status == WishStatusEnum::CLAIMED {
        sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status='FINISHED', updated_at=NOW() WHERE wish_id=$1 RETURNING *"
        )
        .bind(*id)
        .fetch_one(&mut *tx).await?
    } else {
        wish_rec
    };

    tx.commit().await?;

    Ok(HttpResponse::Ok().json(&WishOut::from_record(final_wish_rec, Some(feedback_rec))))
}
