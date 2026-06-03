// 应用服务层 - 签到服务
// 包含签到、连续签到奖励等业务用例

use crate::config::AppState;
use crate::domain::event::{EventType, SignInPayload};
use crate::domain::sign_in::entities::{
    DailyCheckinOut, SignInResponse, SignInfoResponse, SignRecordOut,
};
use crate::domain::user::GroupPointConfig;
use crate::errors::CustomError;
use crate::infrastructure::event::publisher::EventPublisher;
use chrono::{Local, NaiveDate};
use sqlx::Row;
use std::sync::Arc;

/// 签到应用服务
pub struct SignService;

impl SignService {
    /// 用户签到
    pub async fn sign_in(
        user_id: i64,
        state: &Arc<AppState>,
    ) -> Result<SignInResponse, CustomError> {
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        // 检查今日是否已签到
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT sign_id FROM sign_records WHERE user_id = $1 AND sign_date = $2",
        )
        .bind(user_id as i64)
        .bind(today)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            return Err(CustomError::BadRequest("今日已签到".into()));
        }

        // 获取用户组信息
        let group_id: Option<i64> = sqlx::query(
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = true LIMIT 1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?
        .map(|r| r.get("group_id"));

        // 计算连续签到天数
        let yesterday = today.pred_opt().unwrap();
        let last_sign: Option<(NaiveDate, i32)> = sqlx::query_as(
            "SELECT sign_date, consecutive_days FROM sign_records WHERE user_id = $1 AND sign_date = $2"
        )
        .bind(user_id as i64)
        .bind(yesterday)
        .fetch_optional(db)
        .await?;

        let consecutive_days = last_sign.map(|(_, cd)| cd + 1).unwrap_or(1);

        // 获取签到奖励配置
        let sign_reward = if let Some(gid) = group_id {
            let cfg: Option<GroupPointConfig> = sqlx::query_as(
                "SELECT group_id, sign_reward_daily, sign_reward_consecutive, order_point_percent FROM group_point_configs WHERE group_id = $1"
            )
            .bind(gid)
            .fetch_optional(db)
            .await?;
            cfg.map(|c| c.sign_reward_daily).unwrap_or(5)
        } else {
            5 // 默认奖励
        };

        let diamonds_earned = sign_reward;

        // 获取用户当前钻石
        let total_diamonds: i32 = sqlx::query("SELECT diamond FROM users WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        // 插入签到记录
        let sign_id: i64 = sqlx::query_scalar(
            "INSERT INTO sign_records (user_id, sign_date, consecutive_days, diamonds_earned) VALUES ($1, $2, $3, $4) RETURNING sign_id"
        )
        .bind(user_id as i64)
        .bind(today)
        .bind(consecutive_days)
        .bind(diamonds_earned)
        .fetch_one(db)
        .await?;

        // 更新用户钻石
        let new_total = total_diamonds + diamonds_earned;
        sqlx::query("UPDATE users SET diamond = $2 WHERE user_id = $1")
            .bind(user_id as i64)
            .bind(new_total)
            .execute(db)
            .await?;

        // 记录钻石流水
        sqlx::query(
            "INSERT INTO diamond_flow (user_id, group_id, amount, balance, scene) VALUES ($1, $2, $3, $4, 'sign')"
        )
        .bind(user_id as i64)
        .bind(group_id)
        .bind(diamonds_earned)
        .bind(new_total)
        .execute(db)
        .await?;

        // 发布签到事件
        let payload = SignInPayload {
            sign_id,
            user_id,
            group_id,
            sign_date: today.to_string(),
            consecutive_days,
            diamonds_earned,
            trace_id: None,
        };
        let _ = EventPublisher::publish(
            db,
            EventType::SignIn,
            payload,
            Some(user_id),
            group_id,
            Some("sign"),
            Some(sign_id),
        )
        .await;

        let message = if consecutive_days >= 7 {
            "太棒了！连续签到7天！".to_string()
        } else if consecutive_days >= 3 {
            format!("连续签到{}天，继续加油！", consecutive_days)
        } else {
            "签到成功".to_string()
        };

        Ok(SignInResponse {
            sign_id,
            sign_date: today,
            consecutive_days,
            diamonds_earned,
            total_diamonds: new_total,
            message,
        })
    }

    /// 获取签到信息
    pub async fn get_sign_info(
        user_id: i64,
        state: &Arc<AppState>,
    ) -> Result<SignInfoResponse, CustomError> {
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        // 检查今日是否已签到
        let today_sign: Option<(i64, NaiveDate, i32, i32)> = sqlx::query_as(
            "SELECT sign_id, sign_date, consecutive_days, diamonds_earned FROM sign_records WHERE user_id = $1 AND sign_date = $2"
        )
        .bind(user_id as i64)
        .bind(today)
        .fetch_optional(db)
        .await?;

        let (today_signed, today_diamonds) = if let Some((_, _, _, d)) = today_sign {
            (true, d)
        } else {
            (false, 0)
        };

        // 获取最近签到记录
        let recent_rows = sqlx::query(
            "SELECT sign_id, user_id, sign_date, consecutive_days, diamonds_earned, created_at \
             FROM sign_records WHERE user_id = $1 ORDER BY sign_date DESC LIMIT 7",
        )
        .bind(user_id as i64)
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

        // 获取连续签到天数和总签到天数
        let last_sign: Option<(NaiveDate, i32)> = sqlx::query_as(
            "SELECT sign_date, consecutive_days FROM sign_records WHERE user_id = $1 ORDER BY sign_date DESC LIMIT 1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?;

        let consecutive_days = last_sign.map(|(_, cd)| cd).unwrap_or(0);
        let last_sign_date = last_sign.map(|(d, _)| d);

        let total_sign_days: i32 =
            sqlx::query("SELECT COUNT(*) FROM sign_records WHERE user_id = $1")
                .bind(user_id as i64)
                .fetch_one(db)
                .await?
                .get(0);

        Ok(SignInfoResponse {
            today_signed,
            consecutive_days,
            total_sign_days,
            today_diamonds,
            last_sign_date,
            recent_records,
        })
    }

    /// 每日签到
    pub async fn daily_checkin(
        token: crate::middlewares::auth::UserToken,
        state: &Arc<AppState>,
    ) -> Result<DailyCheckinOut, CustomError> {
        let user_id = token.user_id;
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        // 检查今日是否已签到
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT sign_id FROM sign_records WHERE user_id = $1 AND sign_date = $2",
        )
        .bind(user_id as i64)
        .bind(today)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            return Err(CustomError::BadRequest("今日已签到".into()));
        }

        // 获取用户组信息
        let group_id: Option<i64> = sqlx::query(
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = true LIMIT 1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?
        .map(|r| r.get("group_id"));

        // 计算连续签到天数
        let yesterday = today.pred_opt().unwrap();
        let last_sign: Option<(NaiveDate, i32)> = sqlx::query_as(
            "SELECT sign_date, consecutive_days FROM sign_records WHERE user_id = $1 AND sign_date = $2"
        )
        .bind(user_id as i64)
        .bind(yesterday)
        .fetch_optional(db)
        .await?;

        let consecutive_days = last_sign.map(|(_, cd)| cd + 1).unwrap_or(1);

        // 获取签到奖励配置
        let sign_reward = if let Some(gid) = group_id {
            let cfg: Option<GroupPointConfig> = sqlx::query_as(
                "SELECT group_id, sign_reward_daily, sign_reward_consecutive, order_point_percent FROM group_point_configs WHERE group_id = $1"
            )
            .bind(gid)
            .fetch_optional(db)
            .await?;
            cfg.map(|c| c.sign_reward_daily).unwrap_or(5)
        } else {
            5
        };

        let diamonds_earned = sign_reward;

        // 获取用户当前钻石
        let total_diamonds: i32 = sqlx::query("SELECT diamond FROM users WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        // 插入签到记录
        let sign_id: i64 = sqlx::query_scalar(
            "INSERT INTO sign_records (user_id, sign_date, consecutive_days, diamonds_earned) VALUES ($1, $2, $3, $4) RETURNING sign_id"
        )
        .bind(user_id as i64)
        .bind(today)
        .bind(consecutive_days)
        .bind(diamonds_earned)
        .fetch_one(db)
        .await?;

        // 更新用户钻石
        let new_total = total_diamonds + diamonds_earned;
        sqlx::query("UPDATE users SET diamond = $2 WHERE user_id = $1")
            .bind(user_id as i64)
            .bind(new_total)
            .execute(db)
            .await?;

        // 记录钻石流水
        sqlx::query(
            "INSERT INTO diamond_flow (user_id, group_id, amount, balance, scene) VALUES ($1, $2, $3, $4, 'sign')"
        )
        .bind(user_id as i64)
        .bind(group_id)
        .bind(diamonds_earned)
        .bind(new_total)
        .execute(db)
        .await?;

        // 发布签到事件
        let payload = SignInPayload {
            sign_id,
            user_id,
            group_id,
            sign_date: today.to_string(),
            consecutive_days,
            diamonds_earned,
            trace_id: None,
        };
        let _ = EventPublisher::publish(
            db,
            EventType::SignIn,
            payload,
            Some(user_id),
            group_id,
            Some("sign"),
            Some(sign_id),
        )
        .await;

        Ok(DailyCheckinOut {
            diamonds_earned,
            consecutive_days,
            total_diamonds: new_total,
        })
    }
}
