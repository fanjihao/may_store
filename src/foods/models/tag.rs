use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagRecord {
    pub tag_id: i64,
    pub tag_name: String,
    pub icon: Option<String>,
    pub group_id: Option<i64>,
    pub sort: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodTagOut {
    pub tag_id: i64,
    pub tag_name: String,
    pub icon: Option<String>,
    pub sort: Option<i32>,
    pub food_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagCreateInput {
    pub tag_name: String,
    pub icon: Option<String>,
    pub group_id: Option<i64>,
    pub sort: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagUpdateInput {
    pub tag_id: i64,
    pub tag_name: Option<String>,
    pub icon: Option<String>,
    pub sort: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagSortItem {
    pub tag_id: i64,
    pub sort: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchTagSortInput {
    pub items: Vec<TagSortItem>,
}
