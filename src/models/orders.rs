use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderListOut {
    pub order_id: Option<i32>,
    pub order_no: Option<String>,
    pub order_status: Option<i32>,
    pub create_date: Option<DateTime<Utc>>,
    pub create_user_id: Option<i32>,
    pub recv_user_id: Option<i32>,
    pub goal_time: Option<DateTime<Utc>>,
    pub finish_time: Option<DateTime<Utc>>,
    pub remarks: Option<String>,
    pub is_del: Option<i32>,
    pub order_detail: Option<Vec<OrderDetailListOut>>,
}


#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderOut {
    pub order_id: Option<i32>,
    pub order_no: Option<String>,
    pub order_status: Option<i32>,
    pub create_date: Option<DateTime<Utc>>,
    pub create_user_id: Option<i32>,
    pub recv_user_id: Option<i32>,
    pub goal_time: Option<DateTime<Utc>>,
    pub finish_time: Option<DateTime<Utc>>,
    pub remarks: Option<String>,
    pub is_del: Option<i32>,
    pub order_detail: Option<Vec<OrderDetailListOut>>,
    pub revoke_time: Option<DateTime<Utc>>,
    pub approval_time: Option<DateTime<Utc>>,
    pub approval_feedback: Option<String>,
    pub finish_feedback: Option<String>,
    pub approval_status: Option<i32>,
    pub finish_status: Option<i32>,
    pub points: Option<i32>
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderDetailListOut {
    pub order_d_id: Option<i32>,
    pub order_id: Option<i32>,
    pub food_id: Option<i32>,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderListDto {
    pub status: Option<String>,
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderDto {
    pub user_id: i32,
    pub recv_id: Option<i32>,
    pub ship_id: Option<i32>,
    pub goal_time: Option<DateTime<Utc>>,
    pub remarks: Option<String>,
    pub order_child: String,
    pub foods: Option<String>
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateOrder {
    pub status: i32,
    pub id: i32,
    pub order_no: Option<String>,
    pub child_status: Option<i32>,
    pub user_id: Option<i32>,
    pub approval_feedback: Option<String>,
    pub finish_feedback: Option<String>,
    pub points: Option<i32>,
    pub transaction_type: Option<String>,
    pub balance: Option<i32>,
    pub ship_id: Option<i32>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderFootprint {
    pub id: Option<i32>,
    pub ship_id: Option<i32>,
    pub action: Option<String>,
    pub order_no: Option<String>,
    pub foods: Option<String>,
    pub create_at: Option<DateTime<Utc>>
}