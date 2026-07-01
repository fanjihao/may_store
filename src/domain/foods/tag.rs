// 领域层 - 标签实体和 DTO

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 标签记录
/// 注: SQL 中已经 `tag_id AS id` 和 `tag_name AS name`, 所以 sqlx 按字段名 `id`/`name` 匹配即可
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagRecord {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub color: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 标签创建输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagCreateInput {
    pub name: String,
    pub color: Option<String>,
}

/// 标签更新输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagUpdateInput {
    pub name: Option<String>,
    pub color: Option<String>,
}

/// 标签输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodTagOut {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub color: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 批量排序输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchTagSortInput {
    pub sorts: Vec<TagSortItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagSortItem {
    pub id: i64,
    pub sort: i32,
}