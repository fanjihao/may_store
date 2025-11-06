use crate::{
    errors::CustomError,
    models::{
        users::UserToken,
        wishes::{WishClaimCreateInput, WishClaimOut, WishClaimStatusEnum, WishClaimUpdateInput},
    },
    AppState,
};
use chrono::Utc;
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(post, path="/wish_claims", tag="心愿", request_body=WishClaimCreateInput, responses((status=201, body=WishClaimOut)))]
pub async fn claim_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<WishClaimCreateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;
    // 获取心愿
    let wish_row = sqlx::query("SELECT wish_id, wish_name, wish_cost, status, created_by, created_at, updated_at FROM wishes WHERE wish_id=$1 FOR SHARE")
        .bind(data.wish_id)
        .fetch_optional(&mut *tx).await?;
    let Some(wr) = wish_row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    let wish_cost: i32 = wr.get("wish_cost");
    let status_str: String = wr.get::<String, _>("status");
    if status_str != "ON" {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("心愿已关闭".into()));
    }
    // 检查是否已有进行中的兑换
    let existing = sqlx::query(
        "SELECT id FROM wish_claims WHERE wish_id=$1 AND user_id=$2 AND status='PROCESSING'",
    )
    .bind(data.wish_id)
    .bind(user_token.user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing.is_some() {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("已有进行中的兑换".into()));
    }
    // 用户积分锁 & 校验
    let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
        .bind(user_token.user_id)
        .fetch_one(&mut *tx)
        .await?;
    let love_point: i32 = user_row.get("love_point");
    if love_point < wish_cost {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("积分不足".into()));
    }
    let balance_after = love_point - wish_cost;
    // 扣减积分 & 记录流水
    sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
        .bind(user_token.user_id)
        .bind(balance_after)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO point_transactions(user_id, amount, type, ref_type, ref_id, balance_after) VALUES($1,$2,'WISH_COST',2,$3,$4)")
        .bind(user_token.user_id).bind(-wish_cost).bind(data.wish_id).bind(balance_after).execute(&mut *tx).await?;
    // 插入兑换记录
    let claim_row = sqlx::query("INSERT INTO wish_claims (wish_id, user_id, cost, status, remark) VALUES ($1,$2,$3,'PROCESSING',$4) RETURNING id, wish_id, user_id, cost, status, remark, fulfill_at, created_at, updated_at")
        .bind(data.wish_id).bind(user_token.user_id).bind(wish_cost).bind(&data.remark)
        .fetch_one(&mut *tx).await?;
    tx.commit().await?;
    let out = WishClaimOut {
        id: claim_row.get("id"),
        wish_id: claim_row.get("wish_id"),
        user_id: claim_row.get("user_id"),
        cost: claim_row.get("cost"),
        status: WishClaimStatusEnum::PROCESSING,
        remark: claim_row.try_get("remark").ok(),
        fulfill_at: claim_row.try_get("fulfill_at").ok(),
        created_at: claim_row.get("created_at"),
        updated_at: claim_row.get("updated_at"),
    };
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(put, path="/wish_claims/status", tag="心愿", request_body=WishClaimUpdateInput, responses((status=200, body=WishClaimOut)))]
pub async fn update_wish_claim(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<WishClaimUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;
    let row = sqlx::query("SELECT id, wish_id, user_id, cost, status, remark, fulfill_at, created_at, updated_at FROM wish_claims WHERE id=$1 FOR UPDATE")
        .bind(data.claim_id).fetch_optional(&mut *tx).await?;
    let Some(r) = row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("兑换记录不存在".into()));
    };
    let current_status_str: String = r.get("status");
    let current_status = match current_status_str.as_str() {
        "PROCESSING" => WishClaimStatusEnum::PROCESSING,
        "DONE" => WishClaimStatusEnum::DONE,
        "CANCELLED" => WishClaimStatusEnum::CANCELLED,
        _ => WishClaimStatusEnum::PROCESSING,
    };
    if current_status == data.to_status {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("状态未变化".into()));
    }
    if !current_status.can_transition(data.to_status) {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("非法状态流转".into()));
    }
    // 限制：只能本人或心愿创建者操作（此处仅本人，后续可扩展）
    let user_id: i64 = r.get("user_id");
    if user_id != user_token.user_id {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("只能操作自己的兑换".into()));
    }
    match data.to_status {
        WishClaimStatusEnum::DONE => {
            sqlx::query("UPDATE wish_claims SET status='DONE', fulfill_at=NOW(), remark=$2, updated_at=NOW() WHERE id=$1")
            .bind(data.claim_id).bind(&data.remark).execute(&mut *tx).await?;
            // 可选推送
            let wish_id: i64 = r.get("wish_id");
            tx.commit().await?;
            // 推送（简单重用订单模板逻辑）
            let pool_clone = state.db_pool.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    crate::services::notifications::push_order_status(wish_id, pool_clone.clone())
                        .await
                {
                    log::warn!("wish fulfill pseudo push error: {}", e);
                }
            });
            let out = WishClaimOut {
                id: r.get("id"),
                wish_id,
                user_id,
                cost: r.get("cost"),
                status: WishClaimStatusEnum::DONE,
                remark: data.remark.clone(),
                fulfill_at: Some(Utc::now()),
                created_at: r.get("created_at"),
                updated_at: Utc::now(),
            };
            return Ok(HttpResponse::Ok().json(&out));
        }
        WishClaimStatusEnum::CANCELLED => {
            sqlx::query("UPDATE wish_claims SET status='CANCELLED', remark=$2, updated_at=NOW() WHERE id=$1")
            .bind(data.claim_id).bind(&data.remark).execute(&mut *tx).await?;
            tx.commit().await?;
            let out = WishClaimOut {
                id: r.get("id"),
                wish_id: r.get("wish_id"),
                user_id,
                cost: r.get("cost"),
                status: WishClaimStatusEnum::CANCELLED,
                remark: data.remark.clone(),
                fulfill_at: r.try_get("fulfill_at").ok(),
                created_at: r.get("created_at"),
                updated_at: Utc::now(),
            };
            return Ok(HttpResponse::Ok().json(&out));
        }
        _ => {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("不支持的目标状态".into()));
        }
    }
}
