use crate::{
    errors::CustomError,
    models::{
        pagination::{decode_cursor, encode_cursor, CursorPage},
        users::UserToken,
        wishes::{
            WishClaimFeedbackInput, WishClaimOut, WishClaimRecord, WishClaimStatusEnum,
            WishClaimUpdateInput, WishStatusEnum,
        },
    },
    AppState,
};
use chrono::Utc;
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct RedeemInput {
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ClaimCursor {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct WishClaimQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

// 5. Redeem wish
#[utoipa::path(
    post,
    path = "/wishes/{id}/redeem",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = RedeemInput,
    responses((status = 201, body = WishClaimOut))
)]
pub async fn redeem_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<RedeemInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // Get wish
    let wish_row = sqlx::query(
        "SELECT wish_id, wish_name, wish_cost, status, created_by FROM wishes WHERE wish_id=$1 FOR SHARE"
    )
    .bind(*id)
    .fetch_optional(&mut *tx).await?;

    let Some(wr) = wish_row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };

    let wish_cost: i32 = wr.get("wish_cost");
    let status: WishStatusEnum = wr.get("status");
    let created_by: i64 = wr.get("created_by");

    if status != WishStatusEnum::ON {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("心愿已关闭".into()));
    }

    if created_by == user_token.user_id {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("无法兑换自己的心愿".into()));
    }

    // Check for ANY existing processing or done claim (Global uniqueness)
    let existing = sqlx::query(
        "SELECT id FROM wish_claims WHERE wish_id=$1 AND status != 'CANCELLED'"
    )
    .bind(*id)
    .fetch_optional(&mut *tx).await?;

    if existing.is_some() {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("该心愿已被抢先兑换".into()));
    }

    // Check points
    let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
        .bind(user_token.user_id)
        .fetch_one(&mut *tx).await?;
    let love_point: i32 = user_row.get("love_point");

    if love_point < wish_cost {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("积分不足".into()));
    }

    let balance_after = love_point - wish_cost;

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
    .bind(-wish_cost)
    .bind(*id)
    .bind(balance_after)
    .execute(&mut *tx).await?;

    // Create claim
    let claim_row = sqlx::query(
        "INSERT INTO wish_claims (wish_id, user_id, cost, status, remark) VALUES ($1,$2,$3,'PROCESSING',$4) RETURNING *"
    )
    .bind(*id)
    .bind(user_token.user_id)
    .bind(wish_cost)
    .bind(&data.remark)
    .fetch_one(&mut *tx).await?;

    tx.commit().await?;
    let rec = WishClaimRecord::from_row(&claim_row).map_err(|e| CustomError::InternalServerError(e.to_string()))?;

    Ok(HttpResponse::Created().json(&WishClaimOut::from(rec)))
}

// Update claim status (for redemption workflow)
#[utoipa::path(
    put,
    path = "/wish_claims/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "兑换ID")),
    request_body = WishClaimUpdateInput,
    responses((status = 200, body = WishClaimOut))
)]
pub async fn update_claim_status(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<WishClaimUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    let row = sqlx::query(
        "SELECT * FROM wish_claims WHERE id=$1 FOR UPDATE"
    )
    .bind(*id)
    .fetch_optional(&mut *tx).await?;

    let Some(r) = row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };
    let rec_origin = WishClaimRecord::from_row(&r).map_err(|e| CustomError::InternalServerError(e.to_string()))?;

    if rec_origin.status == data.to_status {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("状态未变化".into()));
    }
    if !rec_origin.status.can_transition(data.to_status) {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("非法状态流转".into()));
    }

    if rec_origin.user_id != user_token.user_id {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("只能操作自己的兑换".into()));
    }

    match data.to_status {
        WishClaimStatusEnum::DONE => {
            let row = sqlx::query(
                "UPDATE wish_claims SET status='DONE', fulfill_at=NOW(), remark=$2, updated_at=NOW() WHERE id=$1 RETURNING *"
            )
            .bind(*id)
            .bind(&data.remark)
            .fetch_one(&mut *tx).await?;

            tx.commit().await?;

            // Pseudo push (async)
            let pool_clone = state.db_pool.clone();
            let wish_id = rec_origin.wish_id;
            tokio::spawn(async move {
                if let Err(e) = crate::services::notifications::push_order_status(wish_id, pool_clone).await {
                    log::warn!("wish fulfill push error: {}", e);
                }
            });

            let rec = WishClaimRecord::from_row(&row).map_err(|e| CustomError::InternalServerError(e.to_string()))?;
            Ok(HttpResponse::Ok().json(&WishClaimOut::from(rec)))
        }
        WishClaimStatusEnum::CANCELLED => {
            let row = sqlx::query(
                "UPDATE wish_claims SET status='CANCELLED', remark=$2, updated_at=NOW() WHERE id=$1 RETURNING *"
            )
            .bind(*id)
            .bind(&data.remark)
            .fetch_one(&mut *tx).await?;

            tx.commit().await?;

            let rec = WishClaimRecord::from_row(&row).map_err(|e| CustomError::InternalServerError(e.to_string()))?;
            Ok(HttpResponse::Ok().json(&WishClaimOut::from(rec)))
        }
        _ => {
            tx.rollback().await.ok();
            Err(CustomError::BadRequest("不支持的目标状态".into()))
        }
    }
}

// Get user's claim history
#[utoipa::path(
    get,
    path = "/wish_claims",
    tag = "心愿",
    params(WishClaimQuery),
    responses((status = 200, body = CursorPage<WishClaimOut>))
)]
pub async fn list_my_claims(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<WishClaimQuery>,
) -> Result<impl Responder, CustomError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT * FROM wish_claims WHERE user_id = "
    );
    qb.push_bind(user_token.user_id);

    if let Some(cursor_str) = &query.cursor {
        if let Some(cursor) = decode_cursor::<ClaimCursor>(cursor_str) {
            qb.push(" AND (created_at, id) < (");
            qb.push_bind(cursor.created_at);
            qb.push(", ");
            qb.push_bind(cursor.id);
            qb.push(") ");
        }
    }

    qb.push(" ORDER BY created_at DESC, id DESC LIMIT ");
    qb.push_bind(limit + 1);

    let rows = qb.build().fetch_all(&state.db_pool).await?;

    let mut rows = rows;
    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&ClaimCursor {
                created_at: r.get("created_at"),
                id: r.get("id"),
            })
        })
    } else {
        None
    };

    let items: Vec<WishClaimOut> = rows
        .into_iter()
        .map(|r| {
             let rec = WishClaimRecord::from_row(&r).unwrap(); // safe given query
             WishClaimOut::from(rec)
        })
        .collect();

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
    }))
}

// 6. Get Single Claim (with feedback)
#[utoipa::path(
    get,
    path = "/wish_claims/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "兑换ID")),
    responses((status = 200, body = WishClaimOut))
)]
pub async fn get_claim(
    _user_token: UserToken, // Allow any user to view? Or restrict?
    // Requirement says: Creator can see feedback, Redeemer can edit.
    // Let's allow authenticated users to view for now, usually Group members.
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let row = sqlx::query("SELECT * FROM wish_claims WHERE id=$1")
        .bind(*id)
        .fetch_optional(&state.db_pool).await?;

    let Some(r) = row else {
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };

    let rec = WishClaimRecord::from_row(&r).map_err(|e| CustomError::InternalServerError(e.to_string()))?;
    Ok(HttpResponse::Ok().json(&WishClaimOut::from(rec)))
}

// 7. Submit/Update Feedback
#[utoipa::path(
    put,
    path = "/wish_claims/{id}/feedback",
    tag = "心愿",
    params(("id" = i64, Path, description = "兑换ID")),
    request_body = WishClaimFeedbackInput,
    responses((status = 200, body = WishClaimOut))
)]
pub async fn submit_feedback(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<WishClaimFeedbackInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // Lock row
    let row = sqlx::query("SELECT * FROM wish_claims WHERE id=$1 FOR UPDATE")
        .bind(*id)
        .fetch_optional(&mut *tx).await?;

    let Some(r) = row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };
    let rec = WishClaimRecord::from_row(&r).map_err(|e| CustomError::InternalServerError(e.to_string()))?;

    // Check ownership
    if rec.user_id != user_token.user_id {
         tx.rollback().await.ok();
         return Err(CustomError::BadRequest("只能为自己的兑换提交反馈".into()));
    }

    if rec.status == WishClaimStatusEnum::CANCELLED {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("已取消的兑换无法提交反馈".into()));
    }

    let feedback_at = data.feedback_at.unwrap_or_else(Utc::now);

    // Update fields and ensure status is DONE
    let updated_row = sqlx::query(
        "UPDATE wish_claims SET photo_url=$1, location_text=$2, mood_text=$3, feeling_text=$4, feedback_at=$5, status='DONE', fulfill_at=COALESCE(fulfill_at, NOW()), updated_at=NOW() WHERE id=$6 RETURNING *"
    )
    .bind(&data.photo_url)
    .bind(&data.location_text)
    .bind(&data.mood_text)
    .bind(&data.feeling_text)
    .bind(feedback_at)
    .bind(*id)
    .fetch_one(&mut *tx).await?;

    tx.commit().await?;

    let rec_new = WishClaimRecord::from_row(&updated_row).map_err(|e| CustomError::InternalServerError(e.to_string()))?;
    Ok(HttpResponse::Ok().json(&WishClaimOut::from(rec_new)))
}
