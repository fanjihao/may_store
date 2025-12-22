use crate::{
    errors::CustomError,
    models::users::{DailyCheckinOut, UserToken},
    AppState,
};
use ntex::web::{types::State, HttpResponse, Responder};
use sqlx::Row;
use std::sync::Arc;

const DAILY_CHECKIN_REWARD: i32 = 1;
/// point_transactions.ref_type 的业务自定义值：3 = 每日签到
const REF_TYPE_DAILY_CHECKIN: i16 = 3;

#[utoipa::path(
    post,
    path = "/users/checkin",
    tag = "用户",
    summary = "每日签到获取爱心积分",
    responses(
        (status = 201, body = DailyCheckinOut),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn daily_checkin(
    mut user_token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;

    // 锁定用户积分行，避免并发重复签到/重复加分
    let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
        .bind(user_token.user_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(user_row) = user_row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("用户不存在".into()));
    };

    // 同一天是否已经签到（按数据库 CURRENT_DATE）
    let existing = sqlx::query(
        "SELECT 1 FROM point_transactions WHERE user_id=$1 AND ref_type=$2 AND created_at::date=CURRENT_DATE LIMIT 1",
    )
    .bind(user_token.user_id)
    .bind(REF_TYPE_DAILY_CHECKIN)
    .fetch_optional(&mut *tx)
    .await?;

    if existing.is_some() {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("今日已签到".into()));
    }

    // 用 DB 生成今日 ref_id，避免服务器/DB 时区不一致导致的日期偏差
    let date_row = sqlx::query("SELECT to_char(CURRENT_DATE,'YYYYMMDD')::bigint AS did")
        .fetch_one(&mut *tx)
        .await?;
    let date_id: i64 = date_row.get("did");

    let current_lp: i32 = user_row.get("love_point");
    let balance_after = current_lp + DAILY_CHECKIN_REWARD;

    // 更新积分
    sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
        .bind(user_token.user_id)
        .bind(balance_after)
        .execute(&mut *tx)
        .await?;

    // 记录流水
    sqlx::query(
        "INSERT INTO point_transactions (user_id, amount, type, ref_type, ref_id, balance_after) VALUES ($1,$2,'OTHER',$3,$4,$5)",
    )
    .bind(user_token.user_id)
    .bind(DAILY_CHECKIN_REWARD)
    .bind(REF_TYPE_DAILY_CHECKIN)
    .bind(date_id)
    .bind(balance_after)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 更新缓存（若 token 已带用户信息）
    if let Some(mut public) = user_token.user.take() {
        public.love_point = balance_after;
        let _ = state.redis_cache.set_user_public(&public, 3600).await;
    }

    Ok(HttpResponse::Created().json(&DailyCheckinOut {
        added: DAILY_CHECKIN_REWARD,
        balance_after,
    }))
}
