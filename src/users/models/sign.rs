use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// 签到规则：第1-7天依次5,6,7,8,9,10,20分，循环
/// 如果连续签到满7天，第8天重置为第1天（5分）
pub fn calculate_sign_points(consecutive_days: i32) -> i32 {
    let day_in_cycle = ((consecutive_days - 1) % 7) + 1; // 1-7循环
    match day_in_cycle {
        1 => 5,
        2 => 6,
        3 => 7,
        4 => 8,
        5 => 9,
        6 => 10,
        7 => 20,
        _ => 0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInResponse {
    pub sign_id: i64,
    pub sign_date: NaiveDate,
    pub consecutive_days: i32,
    pub points_earned: i32,
    pub total_points: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignRecordOut {
    pub sign_id: i64,
    pub user_id: i64,
    pub sign_date: NaiveDate,
    pub consecutive_days: i32,
    pub points_earned: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInfoResponse {
    pub today_signed: bool,
    pub consecutive_days: i32,
    pub total_sign_days: i32,
    pub today_points: i32,
    pub last_sign_date: Option<NaiveDate>,
    pub recent_records: Vec<SignRecordOut>,
}
