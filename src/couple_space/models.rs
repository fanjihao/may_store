use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorialDay {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub memorial_date: chrono::NaiveDate,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_default: i16,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorialDayCreate {
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub memorial_date: chrono::NaiveDate,
    pub is_default: Option<i16>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorialDayUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub memorial_date: Option<chrono::NaiveDate>,
    pub is_default: Option<i16>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MemorialDayQuery {
    pub group_id: i64,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemorialDayCursor {
    pub memorial_date: chrono::NaiveDate,
    pub id: i64,
}
