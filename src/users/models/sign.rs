use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// 签到规则：根据配置的奖励数组循环
/// 如果没有提供配置，则使用默认的 [5, 6, 7, 8, 9, 10, 20]
pub fn calculate_sign_diamonds(consecutive_days: i32, rewards: &[i32]) -> i32 {
    if rewards.is_empty() {
        return 0;
    }
    let len = rewards.len() as i32;
    let index = ((consecutive_days - 1) % len) as usize;
    rewards[index]
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInResponse {
    pub sign_id: i64,
    pub sign_date: NaiveDate,
    pub consecutive_days: i32,
    pub diamonds_earned: i32,
    pub total_diamonds: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignRecordOut {
    pub sign_id: i64,
    pub user_id: i64,
    pub sign_date: NaiveDate,
    pub consecutive_days: i32,
    pub diamonds_earned: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInfoResponse {
    pub today_signed: bool,
    pub consecutive_days: i32,
    pub total_sign_days: i32,
    pub today_diamonds: i32,
    pub last_sign_date: Option<NaiveDate>,
    pub recent_records: Vec<SignRecordOut>,
}
