use chrono::{ DateTime, Utc };
use serde::{ Deserialize, Serialize };
use utoipa::ToSchema;

// ================= Enums =================
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishStatusEnum {
    ON,
    OFF,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_claim_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishClaimStatusEnum {
    PROCESSING,
    DONE,
    CANCELLED,
}

// ================= Records =================
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct WishRecord {
    pub wish_id: i64,
    pub wish_name: String,
    pub wish_cost: i32,
    pub status: WishStatusEnum,
    pub created_by: i64, // group_id
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
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
}

// ================= DTOs =================
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishCreateInput {
    pub wish_name: String,
    pub wish_cost: i32,
    pub group_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishUpdateInput {
    pub wish_id: i64,
    pub wish_name: Option<String>,
    pub wish_cost: Option<i32>,
    pub status: Option<WishStatusEnum>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct WishQuery {
    pub status: Option<WishStatusEnum>,
    pub created_by: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishOut {
    pub wish_id: i64,
    pub wish_name: String,
    pub wish_cost: i32,
    pub status: WishStatusEnum,
    pub created_by: i64, // group_id
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub current_claim_status: Option<WishClaimStatusEnum>,
}

impl From<WishRecord> for WishOut {
    fn from(r: WishRecord) -> Self {
        Self {
            wish_id: r.wish_id,
            wish_name: r.wish_name,
            wish_cost: r.wish_cost,
            status: r.status,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            current_claim_status: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishClaimCreateInput {
    pub wish_id: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishClaimUpdateInput {
    pub claim_id: i64,
    pub to_status: WishClaimStatusEnum,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

// ================= Post-Fulfillment Check-in =================
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct WishClaimCheckinRecord {
    pub id: i64,
    pub claim_id: i64,
    pub user_id: i64,
    pub photo_url: Option<String>,
    pub location_text: Option<String>,
    pub mood_text: Option<String>,
    pub feeling_text: Option<String>,
    pub checkin_time: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishClaimCheckinCreateInput {
    pub claim_id: i64,
    pub photo_url: Option<String>,
    pub location_text: Option<String>,
    pub mood_text: Option<String>,
    pub feeling_text: Option<String>,
    pub checkin_time: Option<DateTime<Utc>>, // 客户端可覆盖时间
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishClaimCheckinOut {
    pub id: i64,
    pub claim_id: i64,
    pub user_id: i64,
    pub photo_url: Option<String>,
    pub location_text: Option<String>,
    pub mood_text: Option<String>,
    pub feeling_text: Option<String>,
    pub checkin_time: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<WishClaimCheckinRecord> for WishClaimCheckinOut {
    fn from(r: WishClaimCheckinRecord) -> Self {
        Self {
            id: r.id,
            claim_id: r.claim_id,
            user_id: r.user_id,
            photo_url: r.photo_url,
            location_text: r.location_text,
            mood_text: r.mood_text,
            feeling_text: r.feeling_text,
            checkin_time: r.checkin_time,
            created_at: r.created_at,
        }
    }
}
