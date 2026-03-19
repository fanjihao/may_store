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
    async fn get_rewards_config(group_id: Option<i64>, state: &AppState) -> Vec<i32> {
        let mut rewards = vec![5, 6, 7, 8, 9, 10, 20];
        if let Some(gid) = group_id {
            if let Ok(config) = sqlx::query_scalar::<_, Vec<i32>>(
                "SELECT daily_checkin_rewards FROM group_point_configs WHERE group_id = $1"
            )
            .bind(gid)
            .fetch_one(&state.db_pool)
            .await
            {
                rewards = config;
            }
        }
        rewards
    }

    pub async fn daily_checkin(
        mut user_token: UserToken,
        state: &AppState,
    ) -> Result<DailyCheckinOut, CustomError> {
        let db = &state.db_pool;

        let group_id = user_token.user.as_ref().and_then(|u| u.group_id);
        let rewards = Self::get_rewards_config(group_id, state).await;
        // daily_checkin 默认奖励取第一个值，或者保持原有的 1？
        // 用户提到的是 [5,6,7,8,9,10,20]，这对应连续签到的逻辑。
        // daily_checkin 看起来不计连续天数，这里我们取第一天的奖励或者保持 1。
        // 根据上下文，用户希望配置这个 reward，我将其改为使用配置的第一项，如果没有则用 1。
        let reward = rewards.first().cloned().unwrap_or(DAILY_CHECKIN_REWARD);

        let mut tx = db.begin().await?;

        let user_row = sqlx::query("SELECT diamond FROM users WHERE user_id=$1 FOR UPDATE")
            .bind(user_token.user_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(user_row) = user_row else {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("用户不存在".into()));
        };

        // 检查是否已签到
        let existing = sqlx::query(
            "SELECT 1 FROM sign_records WHERE user_id=$1 AND sign_date=CURRENT_DATE LIMIT 1",
        )
        .bind(user_token.user_id)
        .fetch_optional(&mut *tx)
        .await?;

        if existing.is_some() {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("今日已签到".into()));
        }

        let current_diamond: i32 = user_row.get("diamond");
        let balance_after = current_diamond + reward;

        sqlx::query("UPDATE users SET diamond=$2 WHERE user_id=$1")
            .bind(user_token.user_id)
            .bind(balance_after)
            .execute(&mut *tx)
            .await?;

        // 插入签到记录 (sign_records 包含 diamonds_earned)
        sqlx::query(
            "INSERT INTO sign_records (user_id, sign_date, consecutive_days, diamonds_earned)
             VALUES ($1, CURRENT_DATE, 1, $2)",
        )
        .bind(user_token.user_id)
        .bind(reward)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        if let Some(mut public) = user_token.user.take() {
            public.diamond = balance_after;
            let _ = state.redis_cache.set_user_public(&public, 3600).await;
        }

        Ok(DailyCheckinOut {
            added: reward,
            balance_after,
        })
    }

    pub async fn sign_in(user_id: i64, state: &AppState) -> Result<SignInResponse, CustomError> {
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        // 获取 group_id
        let group_id = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT agm.group_id FROM association_group_members agm \
             JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 \
             WHERE agm.user_id=$1 ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1",
        )
        .bind(user_id)
        .fetch_one(db)
        .await
        .unwrap_or(None);

        let rewards = Self::get_rewards_config(group_id, state).await;

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
                // 如果昨天签到了，且还没超过奖励数组长度，则天数+1；否则（满周期）重置为1
                if last_days >= rewards.len() as i32 {
                    1
                } else {
                    last_days + 1
                }
            } else {
                // 如果昨天没签到（中断），重置为1
                1
            }
        } else {
            1
        };

        let diamonds_earned =
            crate::users::models::sign::calculate_sign_diamonds(consecutive_days, &rewards);

        let mut tx = db.begin().await?;

        let current_diamonds: i32 =
            sqlx::query_scalar("SELECT diamond FROM users WHERE user_id=$1 FOR UPDATE")
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await?;

        let new_balance = current_diamonds + diamonds_earned;

        let sign_id = sqlx::query(
            "INSERT INTO sign_records (user_id, sign_date, consecutive_days, diamonds_earned)
         VALUES ($1, $2, $3, $4) RETURNING sign_id",
        )
        .bind(user_id)
        .bind(today)
        .bind(consecutive_days)
        .bind(diamonds_earned)
        .fetch_one(&mut *tx)
        .await?
        .get::<i64, _>("sign_id");

        sqlx::query("UPDATE users SET diamond = diamond + $1 WHERE user_id = $2")
            .bind(diamonds_earned)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        // 由于不再增加爱心积分，移除 point_transactions 的记录逻辑，或者以后如果有 diamond_transactions 再加
        // 既然签到的不再记录到爱心积分流水，原有的 INSERT INTO point_transactions 移除

        tx.commit().await?;

        let message = if consecutive_days >= rewards.len() as i32 {
            format!(
                "连续签到{}天，获得{}个钻石（满{}天额外奖励）",
                consecutive_days, diamonds_earned, rewards.len()
            )
        } else if consecutive_days == 1 {
            format!("首次签到，获得{}个钻石", diamonds_earned)
        } else {
            format!("连续签到{}天，获得{}个钻石", consecutive_days, diamonds_earned)
        };

        Ok(SignInResponse {
            sign_id,
            sign_date: today,
            consecutive_days,
            diamonds_earned,
            total_diamonds: new_balance,
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
            "SELECT sign_id, user_id, sign_date, consecutive_days, diamonds_earned, created_at
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
            diamonds_earned: r.get("diamonds_earned"),
            created_at: r.get("created_at"),
        });

        let last_sign_row = sqlx::query(
            "SELECT sign_id, user_id, sign_date, consecutive_days, diamonds_earned, created_at
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
            diamonds_earned: r.get("diamonds_earned"),
            created_at: r.get("created_at"),
        });

        let total_sign_days: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sign_records WHERE user_id=$1")
                .bind(user_id)
                .fetch_one(db)
                .await?;

        let recent_rows = sqlx::query(
            "SELECT sign_id, user_id, sign_date, consecutive_days, diamonds_earned, created_at
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
                diamonds_earned: r.get("diamonds_earned"),
                created_at: r.get("created_at"),
            })
            .collect();

        Ok(SignInfoResponse {
            today_signed: today_sign.is_some(),
            consecutive_days: today_sign.as_ref().map(|r| r.consecutive_days).unwrap_or(0),
            total_sign_days: total_sign_days as i32,
            today_diamonds: today_sign.as_ref().map(|r| r.diamonds_earned).unwrap_or(0),
            last_sign_date: last_sign.map(|r| r.sign_date),
            recent_records,
        })
    }
}
