use chrono::{DateTime, Utc};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt::Display;
use std::str::FromStr;
use utoipa::ToSchema;

pub fn deserialize_number_from_string<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr + Deserialize<'de>,
    T::Err: Display,
    D: Deserializer<'de>,
{
    use serde::de::Error;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber<T> {
        String(String),
        Number(T),
    }

    match StringOrNumber::<T>::deserialize(deserializer)? {
        StringOrNumber::String(s) => s.parse::<T>().map_err(Error::custom),
        StringOrNumber::Number(n) => Ok(n),
    }
}

// ============ Database Records ============

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserDiamond {
    pub id: i64,
    pub user_id: i64,
    pub diamond_balance: i32,
    pub total_get: i32,
    pub total_consume: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiamondFlow {
    pub id: i64,
    pub flow_no: String,
    pub user_id: i64,
    pub r#type: i16, // 1: Get, 2: Consume
    pub scene: String,
    pub diamond_num: i32,
    pub balance_after: i32,
    pub relation_id: Option<i64>,
    pub remark: Option<String>,
    pub create_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordGroup {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub group_type: i16, // 1: Default
    pub max_capacity: i32,
    pub current_count: i32,
    pub status: i16, // 0: Disabled, 1: Normal
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserRecord {
    pub id: i64,
    pub group_id: i64,
    pub record_group_id: i64,
    pub user_id: i64,
    pub order_id: Option<i64>,
    pub images: String,
    pub content: Option<String>,
    pub address: Option<String>,
    pub record_time: DateTime<Utc>,
    pub like_count: i32,
    pub comment_count: i32,
    pub is_draft: i16, // 0: Official, 1: Draft
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

// ============ API DTOs ============

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintOverview {
    pub together_days: i32,
    pub total_feedings: i32,
    pub streak_days: i32,
    pub total_records: i32,
    pub streak_progress: f32, // Progress towards 7 days
    pub feeding_text: String, // Identity-specific text
    pub diamond_balance: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordCreateInput {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub record_group_id: i64,
    pub images: Vec<String>,
    pub content: Option<String>,
    pub address: Option<String>,
    pub record_time: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DraftConfirmInput {
    pub draft_id: i64,
    pub is_edit: bool,
    pub content: Option<String>,
    pub images: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapacityExpandInput {
    pub record_group_id: i64,
    pub expand_level: i16, // 1: 50->100, 2: 100->200, 3: 200->inf
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RecordCursor {
    pub record_time: DateTime<Utc>,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct RecordQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordOut {
    #[serde(flatten)]
    pub base: UserRecord,
    pub user_nick_name: Option<String>,
    pub user_avatar: Option<String>,
    pub is_liked: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckPermissionResponse {
    pub has_group: bool,
    pub group_id: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitRecordResponse {
    pub record_id: i64,
}
