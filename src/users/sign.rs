use crate::{
    errors::CustomError,
    models::sign::{SignInfoResponse, SignInResponse, SignRecordOut},
    models::users::UserToken,
    AppState,
};
use chrono::Local;
use ntex::web::{types::State, HttpResponse, Responder};
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
    post,
    path = "/sign",
    tag = "签到",
    responses(
        (status = 200, body = SignInResponse),
        (status = 400, description = "今日已签到"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn sign_in(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id as i64;

    // 获取今天日期（本地时间）
    let today = Local::now().date_naive();

    // 检查今天是否已签到
    let existing_sign = sqlx::query(
        "SELECT sign_id FROM sign_records WHERE user_id=$1 AND sign_date=$2"
    )
    .bind(user_id)
    .bind(today)
    .fetch_optional(db)
    .await?;

    if existing_sign.is_some() {
        return Err(CustomError::bad_request("今日已签到，请明天再来"));
    }

    // 获取最近一次签到记录，计算连续天数
    let last_sign = sqlx::query_as::<_, (i32,)>(
        "SELECT consecutive_days FROM sign_records WHERE user_id=$1 ORDER BY sign_date DESC LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    // 计算连续天数
    let consecutive_days = if let Some((last_days,)) = last_sign {
        // 检查是否是昨天（连续签到）
        let yesterday = today.pred_opt().unwrap();

        // 获取昨天是否有签到记录
        let yesterday_sign = sqlx::query(
            "SELECT 1 FROM sign_records WHERE user_id=$1 AND sign_date=$2"
        )
        .bind(user_id)
        .bind(yesterday)
        .fetch_optional(db)
        .await?;

        if yesterday_sign.is_some() {
            // 连续签到，加1，满7天后重置为1
            if last_days >= 7 {
                1
            } else {
                last_days + 1
            }
        } else {
            1 // 断签了，重新开始
        }
    } else {
        1 // 第一次签到
    };

    // 计算本次获得积分
    let points_earned = crate::models::sign::calculate_sign_points(consecutive_days);

    // 开始事务
    let mut tx = db.begin().await?;

    // 获取用户当前积分
    let current_points: i32 = sqlx::query_scalar("SELECT love_point FROM users WHERE user_id=$1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

    let new_balance = current_points + points_earned;

    // 插入签到记录
    let sign_id = sqlx::query(
        "INSERT INTO sign_records (user_id, sign_date, consecutive_days, points_earned)
         VALUES ($1, $2, $3, $4) RETURNING sign_id"
    )
    .bind(user_id)
    .bind(today)
    .bind(consecutive_days)
    .bind(points_earned)
    .fetch_one(&mut *tx)
    .await?
    .get::<i64, _>("sign_id");

    // 更新用户积分
    sqlx::query("UPDATE users SET love_point = love_point + $1 WHERE user_id = $2")
        .bind(points_earned)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    // 插入积分流水
    sqlx::query(
        "INSERT INTO point_transactions (user_id, amount, type, ref_type, ref_id, balance_after)
         VALUES ($1, $2, 'SIGN_IN_REWARD', 1, $3, $4)"
    )
    .bind(user_id)
    .bind(points_earned)
    .bind(sign_id)
    .bind(new_balance)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 构建签到消息
    let message = if consecutive_days >= 7 {
        format!("连续签到{}天，获得{}积分（满7天额外奖励）", consecutive_days, points_earned)
    } else if consecutive_days == 1 {
        format!("首次签到，获得{}积分", points_earned)
    } else {
        format!("连续签到{}天，获得{}积分", consecutive_days, points_earned)
    };

    Ok(HttpResponse::Ok().json(&SignInResponse {
        sign_id,
        sign_date: today,
        consecutive_days,
        points_earned,
        total_points: new_balance,
        message,
    }))
}

#[utoipa::path(
    get,
    path = "/sign/info",
    tag = "签到",
    responses(
        (status = 200, body = SignInfoResponse),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_sign_info(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id as i64;

    // 获取今天日期
    let today = Local::now().date_naive();

    // 检查今天是否已签到
    let today_sign_row = sqlx::query(
        "SELECT sign_id, user_id, sign_date, consecutive_days, points_earned, created_at
         FROM sign_records WHERE user_id=$1 AND sign_date=$2"
    )
    .bind(user_id)
    .bind(today)
    .fetch_optional(db)
    .await?;

    let today_sign = today_sign_row.as_ref().map(|r| SignRecordOut {
        sign_id: r.get("sign_id"),
        user_id: r.get("user_id"),
        sign_date: r.get("sign_date"),
        consecutive_days: r.get("consecutive_days"),
        points_earned: r.get("points_earned"),
        created_at: r.get("created_at"),
    });

    // 获取最近一条签到记录
    let last_sign_row = sqlx::query(
        "SELECT sign_id, user_id, sign_date, consecutive_days, points_earned, created_at
         FROM sign_records WHERE user_id=$1 ORDER BY sign_date DESC, sign_id DESC LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    let last_sign = last_sign_row.as_ref().map(|r| SignRecordOut {
        sign_id: r.get("sign_id"),
        user_id: r.get("user_id"),
        sign_date: r.get("sign_date"),
        consecutive_days: r.get("consecutive_days"),
        points_earned: r.get("points_earned"),
        created_at: r.get("created_at"),
    });

    // 获取总签到天数
    let total_sign_days: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sign_records WHERE user_id=$1"
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    // 获取最近7天签到记录
    let recent_rows = sqlx::query(
        "SELECT sign_id, user_id, sign_date, consecutive_days, points_earned, created_at
         FROM sign_records WHERE user_id=$1 ORDER BY sign_date DESC, sign_id DESC LIMIT 7"
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let recent_records: Vec<SignRecordOut> = recent_rows
        .into_iter()
        .map(|r| SignRecordOut {
            sign_id: r.get("sign_id"),
            user_id: r.get("user_id"),
            sign_date: r.get("sign_date"),
            consecutive_days: r.get("consecutive_days"),
            points_earned: r.get("points_earned"),
            created_at: r.get("created_at"),
        })
        .collect();

    Ok(HttpResponse::Ok().json(&SignInfoResponse {
        today_signed: today_sign.is_some(),
        consecutive_days: today_sign.as_ref().map(|r| r.consecutive_days).unwrap_or(0),
        total_sign_days: total_sign_days as i32,
        today_points: today_sign.as_ref().map(|r| r.points_earned).unwrap_or(0),
        last_sign_date: last_sign.map(|r| r.sign_date),
        recent_records,
    }))
}
