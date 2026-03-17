use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientRecord {
    #[serde(rename = "ingredientId")]
    pub ingredient_id: i64,
    pub name: String,
    pub group_id: Option<i64>,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientOut {
    #[serde(rename = "ingredientId")]
    pub ingredient_id: i64,
    pub name: String,
    pub group_id: Option<i64>,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientCreateInput {
    #[serde(rename = "ingredientName")]
    pub name: String,
    pub group_id: Option<i64>,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientUpdateInput {
    #[serde(rename = "ingredientId")]
    pub ingredient_id: i64,
    pub name: Option<String>,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientSortItem {
    #[serde(rename = "ingredientId")]
    pub ingredient_id: i64,
    pub sort: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchIngredientSortInput {
    pub items: Vec<IngredientSortItem>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IngredientCursor {
    pub sort: Option<i32>,
    pub name: String,
    pub ingredient_id: i64,
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct IngredientQuery {
    pub group_id: Option<i64>,
    pub keyword: Option<String>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}
