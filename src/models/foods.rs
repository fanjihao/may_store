use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoodApply {
    pub user_id: i32,
    pub status: i32,
    pub food_type: Option<i32>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}