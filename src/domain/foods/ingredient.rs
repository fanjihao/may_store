// 领域层 - 食材实体和 DTO

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 食材记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientRecord {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 食材创建输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientCreateInput {
    pub name: String,
    pub icon: Option<String>,
}

/// 食材更新输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientUpdateInput {
    pub name: Option<String>,
    pub icon: Option<String>,
}

/// 食材查询
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct IngredientQuery {
    pub group_id: Option<i64>,
    pub keyword: Option<String>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

/// 食材输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientOut {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 批量排序输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchIngredientSortInput {
    pub items: Vec<IngredientSortItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientSortItem {
    pub ingredient_id: i64,
    pub sort: i32,
}