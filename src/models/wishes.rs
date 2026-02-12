use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use ntex::web::types::Query;

// ================= Enums =================
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishStatusEnum {
    ON,
    OFF,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[sqlx(
    type_name = "wish_claim_status_enum",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum WishClaimStatusEnum {
    PROCESSING,
    DONE,
    CANCELLED,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WishClaimRecord {
    pub id: i64,
    pub wish_id: i64,
    pub user_id: i64,
    pub cost: i32,
    pub status: WishClaimStatusEnum,
    pub remark: Option<String>,
    pub fulfill_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Feedback Fields
    pub photo_url: Option<String>,
    pub location_text: Option<String>,
    pub mood_text: Option<String>,
    pub feeling_text: Option<String>,
    pub feedback_at: Option<DateTime<Utc>>,
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
    pub status: Option<WishStatusEnum>,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Enriched fields
    pub claim_status: Option<WishClaimStatusEnum>,
    pub claimant_id: Option<i64>,
}

impl From<WishRecord> for WishOut {
    fn from(r: WishRecord) -> Self {
        Self {
            wish_id: r.wish_id,
            wish_name: r.wish_name,
            wish_cost: r.wish_cost,
            status: r.status,
            created_by: r.created_by,
            group_id: r.group_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            claim_status: None,
            claimant_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct WishQuery {
    pub group_id: Option<i64>,
    pub status: Option<WishStatusEnum>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishClaimCreateInput {
    pub wish_id: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishClaimUpdateInput {
    pub to_status: WishClaimStatusEnum,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishClaimOut {
    pub id: i64,
    pub wish_id: i64,
    pub user_id: i64,
    pub cost: i32,
    pub status: WishClaimStatusEnum,
    pub remark: Option<String>,
    pub fulfill_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Feedback
    pub photo_url: Option<String>,
    pub location_text: Option<String>,
    pub mood_text: Option<String>,
    pub feeling_text: Option<String>,
    pub feedback_at: Option<DateTime<Utc>>,
}

impl From<WishClaimRecord> for WishClaimOut {
    fn from(r: WishClaimRecord) -> Self {
        Self {
            id: r.id,
            wish_id: r.wish_id,
            user_id: r.user_id,
            cost: r.cost,
            status: r.status,
            remark: r.remark,
            fulfill_at: r.fulfill_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            photo_url: r.photo_url,
            location_text: r.location_text,
            mood_text: r.mood_text,
            feeling_text: r.feeling_text,
            feedback_at: r.feedback_at,
        }
    }
}

// ================= Transition Helpers =================
impl WishClaimStatusEnum {
    pub fn can_transition(self, to: WishClaimStatusEnum) -> bool {
        use WishClaimStatusEnum::*;
        match (self, to) {
            (PROCESSING, DONE | CANCELLED) => true,
            (DONE, _) => false,
            (CANCELLED, _) => false,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishClaimFeedbackInput {
    pub photo_url: Option<String>,
    pub location_text: Option<String>,
    pub mood_text: Option<String>,
    pub feeling_text: Option<String>,
    pub feedback_at: Option<DateTime<Utc>>,
}
