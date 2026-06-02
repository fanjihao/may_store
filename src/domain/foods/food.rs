// 领域层 - 菜品实体和 DTO

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 菜品状态枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "food_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FoodStatus {
    Active,
    Inactive,
    Deleted,
}

/// 标记类型枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "mark_type_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarkTypeEnum {
    Like,
    Hate,
    Done,
    Retry,
}

/// 菜品记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodRecord {
    pub food_id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub price: i32,
    pub status: FoodStatus,
    pub created_by: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 菜品创建输入
#[allow(dead_code)]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodCreateInput {
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub images: Vec<String>,
    pub tags: Vec<i64>,
    pub price: i32,
}

/// 菜品更新输入
#[allow(dead_code)]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodUpdateInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub images: Option<Vec<String>>,
    pub tags: Option<Vec<i64>>,
    pub price: Option<i32>,
}

/// 菜品过滤查询
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct FoodFilterQuery {
    pub group_id: Option<i64>,
    pub keyword: Option<String>,
    pub tags: Option<Vec<i64>>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

/// 菜品输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodOut {
    pub food_id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub price: i32,
    pub status: FoodStatus,
    pub created_by: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_liked: bool,
    pub is_done: bool,
}

impl FoodOut {
    pub fn from_record(r: FoodRecord, is_liked: bool, is_done: bool) -> Self {
        Self {
            food_id: r.food_id,
            group_id: r.group_id,
            name: r.name,
            description: r.description,
            images: r.images,
            tags: r.tags,
            price: r.price,
            status: r.status,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            is_liked,
            is_done,
        }
    }
}

/// 标记操作输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodMarkActionInput {
    pub food_id: i64,
    pub mark_type: MarkTypeEnum,
}

/// 盲盒抽取输入
#[allow(dead_code)]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxDrawInput {
    pub group_id: i64,
    pub exclude_ids: Option<Vec<i64>>,
}

/// 盲盒抽取结果
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxDrawResultOut {
    pub food: FoodOut,
}