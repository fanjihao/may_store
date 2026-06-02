// 领域层 - 情侣空间实体

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 纪念日记录
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorialDay {
    pub id: i64,
    pub user_id: i64,
    pub couple_user_id: i64,
    pub name: String,
    pub date: NaiveDate,
    pub day_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 纪念日创建输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorialDayCreate {
    pub name: String,
    pub date: NaiveDate,
    pub day_type: Option<String>,
}

/// 纪念日更新输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorialDayUpdate {
    pub name: Option<String>,
    pub date: Option<NaiveDate>,
    pub day_type: Option<String>,
}

/// 纪念日查询
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct MemorialDayQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

/// 纪念日游标
#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct MemorialDayCursor {
    pub date: NaiveDate,
    pub id: i64,
}
