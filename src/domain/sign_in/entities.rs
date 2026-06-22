// 领域层 - 签到实体
// 包含签到记录和 DTO
// FSD v2026-06-03 §9.1 compliant

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 签到响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInResponse {
    pub sign_id: i64,
    pub sign_date: NaiveDate,
    pub consecutive_days: i32,
    pub diamond_reward: i32,
    pub total_diamonds: i32,
    pub message: String,
}

/// 签到记录输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignRecordOut {
    pub sign_id: i64,
    pub user_id: i64,
    pub sign_date: NaiveDate,
    pub consecutive_days: i32,
    pub diamond_reward: i32,
    pub created_at: DateTime<Utc>,
}

/// 签到信息响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInfoResponse {
    pub today_signed: bool,
    pub consecutive_days: i32,
    pub total_sign_days: i32,
    pub today_diamonds: i32,
    pub last_sign_date: Option<NaiveDate>,
    pub recent_records: Vec<SignRecordOut>,
}

/// 每日签到输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyCheckinOut {
    pub diamond_reward: i32,
    pub consecutive_days: i32,
    pub total_diamonds: i32,
    /// 本次签到是否拿到满签奖励
    pub full_team_bonus: bool,
    /// 满签奖励金额（0 表示没拿到）
    pub full_team_bonus_amt: i32,
}
