use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderListOut {
    pub order_id: Option<i32>,
    pub order_no: Option<String>,
    pub order_status: Option<i32>,
    pub order_c_status: Option<i32>,
    pub create_date: Option<chrono::DateTime<chrono::Utc>>,
    pub create_user_id: Option<i32>,
    pub recv_user_id: Option<i32>,
    pub goal_time: Option<chrono::DateTime<chrono::Utc>>,
    pub finish_time: Option<chrono::DateTime<chrono::Utc>>,
    pub remarks: Option<String>,
    pub is_del: Option<i32>,
    pub order_detail: Option<Vec<OrderDetailListOut>>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDetailListOut {
    pub order_d_id: Option<i32>,
    pub order_id: Option<i32>,
    pub food_id: Option<i32>,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderListDto {
    pub status: Option<i32>,
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDto {
    pub user_id: i32,
    pub recv_id: Option<i32>,
    pub goal_time: Option<chrono::DateTime<chrono::Utc>>,
    pub remarks: Option<String>,
    pub order_child: String
}