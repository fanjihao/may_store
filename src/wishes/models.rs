use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
// use ntex::web::types::Query;
use sqlx::types::Json;

// ================= Enums =================
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishStatusEnum {
    CREATED,
    CLAIMED,
    FINISHED,
    CLOSED,
}

// ================= Records =================
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WishRecord {
    pub wish_id: i64,
    pub wish_name: String,
    pub wish_cost: i32,
    pub status: WishStatusEnum,
    pub created_by: i64, // user_id
    pub group_id: i64,   // group_id
    pub claimed_by: Option<i64>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_cost: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WishFeedbackRecord {
    pub feedback_id: i64,
    pub wish_id: i64,
    pub user_id: i64,
    pub content: Option<String>,
    #[schema(value_type = Option<Vec<String>>)]
    pub images: Option<Json<Vec<String>>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ================= Inputs & Outputs =================

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishCreateInput {
    pub wish_name: String,
    pub wish_cost: i32,
    pub group_id: i64,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishUpdateInput {
    pub wish_name: Option<String>,
    pub wish_cost: Option<i32>,
    pub status: Option<WishStatusEnum>, // Creator can set to CLOSED to delete
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishFeedbackOut {
    pub feedback_id: i64,
    pub user_id: i64,
    pub content: Option<String>,
    pub images: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<WishFeedbackRecord> for WishFeedbackOut {
    fn from(r: WishFeedbackRecord) -> Self {
        Self {
            feedback_id: r.feedback_id,
            user_id: r.user_id,
            content: r.content,
            images: r.images.map(|j| j.0),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishOut {
    pub wish_id: i64,
    pub wish_name: String,
    pub wish_cost: i32,
    pub status: WishStatusEnum,
    pub created_by: i64,
    pub group_id: i64,
    pub claimed_by: Option<i64>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_cost: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Enriched fields
    pub feedback: Option<WishFeedbackOut>,
}

impl WishOut {
    pub fn from_record(r: WishRecord, f: Option<WishFeedbackRecord>) -> Self {
        Self {
            wish_id: r.wish_id,
            wish_name: r.wish_name,
            wish_cost: r.wish_cost,
            status: r.status,
            created_by: r.created_by,
            group_id: r.group_id,
            claimed_by: r.claimed_by,
            claimed_at: r.claimed_at,
            claim_cost: r.claim_cost,
            created_at: r.created_at,
            updated_at: r.updated_at,
            feedback: f.map(WishFeedbackOut::from),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct WishQuery {
    pub group_id: Option<i64>,
    pub status: Option<WishStatusEnum>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishFeedbackInput {
    pub content: Option<String>,
    pub images: Option<Vec<String>>,
}
