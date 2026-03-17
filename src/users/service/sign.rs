use crate::{
    config::AppState,
    errors::CustomError,
    users::models::sign::{SignInResponse, SignInfoResponse, SignRecordOut},
    users::models::user::{DailyCheckinOut, UserToken},
};
use chrono::Local;
use sqlx::Row;

const DAILY_CHECKIN_REWARD: i32 = 1;
const REF_TYPE_DAILY_CHECKIN: i16 = 3;

pub struct SignService;

impl SignService {
    pub async fn daily_checkin(
        mut user_token: UserToken,
        state: &AppState,
    ) -> Result<DailyCheckinOut, CustomError> {
        let db = &state.db_pool;
        let mut tx = db.begin().await?;

        let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
            .bind(user_token.user_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(user_row) = user_row else {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("用户不存在".into()));
        };

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

        let date_row = sqlx::query("SELECT to_char(CURRENT_DATE,'YYYYMMDD')::bigint AS did")
            .fetch_one(&mut *tx)
            .await?;
        let date_id: i64 = date_row.get("did");

        let current_lp: i32 = user_row.get("love_point");
        let balance_after = current_lp + DAILY_CHECKIN_REWARD;

        sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
            .bind(user_token.user_id)
            .bind(balance_after)
            .execute(&mut *tx)
            .await?;

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

        if let Some(mut public) = user_token.user.take() {
            public.love_point = balance_after;
            let _ = state.redis_cache.set_user_public(&public, 3600).await;
        }

        Ok(DailyCheckinOut {
            added: DAILY_CHECKIN_REWARD,
            balance_after,
        })
    }

    pub async fn sign_in(user_id: i64, state: &AppState) -> Result<SignInResponse, CustomError> {
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        let existing_sign =
            sqlx::query("SELECT sign_id FROM sign_records WHERE user_id=$1 AND sign_date=$2")
                .bind(user_id)
                .bind(today)
                .fetch_optional(db)
                .await?;

        if existing_sign.is_some() {
            return Err(CustomError::bad_request("今日已签到，请明天再来"));
        }

        let last_sign = sqlx::query_as::<_, (i32,)>(
        "SELECT consecutive_days FROM sign_records WHERE user_id=$1 ORDER BY sign_date DESC LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

        let consecutive_days = if let Some((last_days,)) = last_sign {
            let yesterday = today.pred_opt().unwrap();
            let yesterday_sign =
                sqlx::query("SELECT 1 FROM sign_records WHERE user_id=$1 AND sign_date=$2")
                    .bind(user_id)
                    .bind(yesterday)
                    .fetch_optional(db)
                    .await?;

            if yesterday_sign.is_some() {
                if last_days >= 7 {
                    1
                } else {
                    last_days + 1
                }
            } else {
                1
            }
        } else {
            1
        };

        let points_earned = crate::users::models::sign::calculate_sign_points(consecutive_days);

        let mut tx = db.begin().await?;

        let current_points: i32 =
            sqlx::query_scalar("SELECT love_point FROM users WHERE user_id=$1")
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await?;

        let new_balance = current_points + points_earned;

        let sign_id = sqlx::query(
            "INSERT INTO sign_records (user_id, sign_date, consecutive_days, points_earned)
         VALUES ($1, $2, $3, $4) RETURNING sign_id",
        )
        .bind(user_id)
        .bind(today)
        .bind(consecutive_days)
        .bind(points_earned)
        .fetch_one(&mut *tx)
        .await?
        .get::<i64, _>("sign_id");

        sqlx::query("UPDATE users SET love_point = love_point + $1 WHERE user_id = $2")
            .bind(points_earned)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
        "INSERT INTO point_transactions (user_id, amount, type, ref_type, ref_id, balance_after)
         VALUES ($1, $2, 'SIGN_IN_REWARD', 1, $3, $4)",
    )
    .bind(user_id)
    .bind(points_earned)
    .bind(sign_id)
    .bind(new_balance)
    .execute(&mut *tx)
    .await?;

        tx.commit().await?;

        let message = if consecutive_days >= 7 {
            format!(
                "连续签到{}天，获得{}积分（满7天额外奖励）",
                consecutive_days, points_earned
            )
        } else if consecutive_days == 1 {
            format!("首次签到，获得{}积分", points_earned)
        } else {
            format!("连续签到{}天，获得{}积分", consecutive_days, points_earned)
        };

        Ok(SignInResponse {
            sign_id,
            sign_date: today,
            consecutive_days,
            points_earned,
            total_points: new_balance,
            message,
        })
    }

    pub async fn get_sign_info(
        user_id: i64,
        state: &AppState,
    ) -> Result<SignInfoResponse, CustomError> {
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        let today_sign_row = sqlx::query(
            "SELECT sign_id, user_id, sign_date, consecutive_days, points_earned, created_at
         FROM sign_records WHERE user_id=$1 AND sign_date=$2",
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

        let last_sign_row = sqlx::query(
            "SELECT sign_id, user_id, sign_date, consecutive_days, points_earned, created_at
         FROM sign_records WHERE user_id=$1 ORDER BY sign_date DESC, sign_id DESC LIMIT 1",
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

        let total_sign_days: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sign_records WHERE user_id=$1")
                .bind(user_id)
                .fetch_one(db)
                .await?;

        let recent_rows = sqlx::query(
            "SELECT sign_id, user_id, sign_date, consecutive_days, points_earned, created_at
         FROM sign_records WHERE user_id=$1 ORDER BY sign_date DESC, sign_id DESC LIMIT 7",
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

        Ok(SignInfoResponse {
            today_signed: today_sign.is_some(),
            consecutive_days: today_sign.as_ref().map(|r| r.consecutive_days).unwrap_or(0),
            total_sign_days: total_sign_days as i32,
            today_points: today_sign.as_ref().map(|r| r.points_earned).unwrap_or(0),
            last_sign_date: last_sign.map(|r| r.sign_date),
            recent_records,
        })
    }
}
