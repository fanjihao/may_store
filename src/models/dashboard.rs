use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCollectOut {
    pub total_orders: Option<i64>,
    pub completed_orders: Option<i64>,
    pub rejected_orders: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRankingDto {
    pub user_id: i32,
    pub role: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayOrderOut {
    pub order_id: i32,
    pub goal_time: Option<chrono::DateTime<chrono::Utc>>,
    pub food_id: Option<i32>,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
}