use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Invitation {
    pub ship_id: Option<i32>,
    pub user_id: Option<i32>,
    pub bind_id: Option<i32>,
    pub ship_status: Option<i32>,
    pub bind_date: Option<chrono::NaiveDate>,
    pub send_avatar: Option<String>,
    pub send_name: Option<String>,
    pub send_role: Option<i32>,
    pub bind_avatar: Option<String>,
    pub bind_name: Option<String>,
    pub bind_role: Option<i32>,
    pub update_date: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindStruct {
    pub bind_id: i32,
    pub user_id: i32
}