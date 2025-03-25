use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodApply {
    pub user_id: i32,
    pub status: String,
    pub food_type: Option<i32>,
    pub food_id: Option<i32>,
    pub food_status: Option<i32>
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateFood {
    pub food_id: i32,
    pub food_status: i32,
    pub msg: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewFood {
    pub user_id: i32,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub food_tags: Option<String>,
    pub food_types: Option<i32>,
    pub food_status: Option<i32>,
    pub food_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodApplyStruct {
    pub food_id: Option<i32>,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub food_tags: Option<String>,
    pub food_types: Option<i32>,
    pub class_name: Option<String>,
    pub food_status: Option<i32>,
    pub food_reason: Option<String>,
    pub create_time: Option<chrono::DateTime<chrono::Utc>>,
    pub finish_time: Option<chrono::DateTime<chrono::Utc>>,
    pub is_mark: Option<i32>,
    pub is_del: Option<i32>,
    pub user_id: Option<i32>,
    pub apply_remarks: Option<String>,
    pub last_order_time: Option<chrono::DateTime<chrono::Utc>>,
    pub last_complete_time: Option<chrono::DateTime<chrono::Utc>>,
    pub total_order_count: Option<i64>,
    pub completed_order_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShowClass {
    pub create_time: Option<chrono::DateTime<chrono::Utc>>,
    pub class_id: Option<i32>,
    pub class_name: Option<String>,
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodTags {
    pub tag_id: Option<i32>,
    pub tag_name: Option<String>,
    pub user_id: Option<i32>,
    pub sort: Option<i32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DishesByType {
    pub user_id: i32,
    pub associate_id: Option<i32>,
    pub class_id: Option<i32>,
    pub is_mark: Option<i32>,
    pub tags: Option<String>,
    pub food_types: Option<i32>,
}