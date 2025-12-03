use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishedListOut {
    pub id: Option<i32>,
    pub wish_name: Option<String>,
    pub wish_cost: Option<i32>,
    pub wish_photo: Option<String>,
    pub wish_location: Option<String>,
    pub wish_date: Option<chrono::DateTime<Utc>>,
    pub create_time: Option<chrono::DateTime<Utc>>,
    pub mood: Option<String>,
    pub create_by: Option<i32>,
    pub exchange_status: Option<i32>
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WishCostDto {
    pub id: i32,
    pub user_id: Option<i32>
}