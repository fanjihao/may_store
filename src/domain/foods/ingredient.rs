// 领域层 - 食材实体和 DTO

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 食材记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientRecord {
    #[sqlx(rename = "ingredient_id")]
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
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
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
}

/// 食材更新输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientUpdateInput {
    pub name: Option<String>,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
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
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingredient_create_input_deserializes_all_fields() {
        let json = r#"{
            "name": "鸡蛋",
            "unit": "个",
            "calories": 60,
            "icon": "https://example.com/egg.png",
            "description": "本地土鸡蛋"
        }"#;
        let input: IngredientCreateInput = serde_json::from_str(json).expect("must parse");
        assert_eq!(input.name, "鸡蛋");
        assert_eq!(input.unit.as_deref(), Some("个"));
        assert_eq!(input.calories, Some(60));
        assert_eq!(input.icon.as_deref(), Some("https://example.com/egg.png"));
        assert_eq!(input.description.as_deref(), Some("本地土鸡蛋"));
    }

    #[test]
    fn ingredient_create_input_minimal_only_name() {
        let json = r#"{"name": "盐"}"#;
        let input: IngredientCreateInput = serde_json::from_str(json).expect("must parse");
        assert_eq!(input.name, "盐");
        assert!(input.unit.is_none());
        assert!(input.calories.is_none());
        assert!(input.icon.is_none());
        assert!(input.description.is_none());
    }
}