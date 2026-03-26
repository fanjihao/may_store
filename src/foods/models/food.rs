use super::ingredient::IngredientOut;
use super::tag::{FoodTagOut, TagRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "food_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FoodStatusEnum {
    NORMAL,
    OFF,
    AUDITING,
    REJECTED,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "submit_role_enum")]
pub enum SubmitRoleEnum {
    #[sqlx(rename = "ORDERING_APPLY")]
    #[serde(rename = "ORDERING_APPLY")]
    OrderingApply,
    #[sqlx(rename = "RECEIVING_CREATE")]
    #[serde(rename = "RECEIVING_CREATE")]
    ReceivingCreate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "apply_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApplyStatusEnum {
    PENDING,
    APPROVED,
    REJECTED,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "mark_type_enum", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarkTypeEnum {
    #[serde(alias = "like")]
    LIKE,
    #[serde(rename = "NOT_RECOMMEND")]
    NotRecommend,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodRecord {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub tag_id: Option<i64>,
    pub ingredients: Option<String>,
    pub steps: Option<String>,
    pub food_status: FoodStatusEnum,
    pub submit_role: SubmitRoleEnum,
    pub apply_status: ApplyStatusEnum,
    pub apply_remark: Option<String>,
    pub created_by: i64,
    pub owner_user_id: Option<i64>,
    pub group_id: Option<i64>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<i64>,
    pub is_del: i16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodOut {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub ingredients: Vec<IngredientOut>,
    pub steps: Option<String>,
    pub food_status: FoodStatusEnum,
    pub apply_status: ApplyStatusEnum,
    pub submit_role: SubmitRoleEnum,
    pub apply_remark: Option<String>,
    pub tag: Option<FoodTagOut>,
    pub is_marked_like: bool,
    pub is_marked_not_recommend: bool,
    pub total_order_count: i32,
    pub completed_order_count: i32,
    pub last_order_time: Option<DateTime<Utc>>,
    pub last_complete_time: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<(FoodRecord, Option<TagRecord>, Vec<MarkTypeEnum>)> for FoodOut {
    fn from(value: (FoodRecord, Option<TagRecord>, Vec<MarkTypeEnum>)) -> Self {
        let (f, tag, marks) = value;
        let like = marks.iter().any(|m| matches!(m, MarkTypeEnum::LIKE));
        let not_rec = marks
            .iter()
            .any(|m| matches!(m, MarkTypeEnum::NotRecommend));
        Self {
            food_id: f.food_id,
            food_name: f.food_name,
            food_photo: f.food_photo,
            ingredients: Vec::new(),
            steps: f.steps,
            food_status: f.food_status,
            apply_status: f.apply_status,
            submit_role: f.submit_role,
            apply_remark: f.apply_remark,
            tag: tag.map(|t| FoodTagOut {
                tag_id: t.tag_id,
                tag_name: t.tag_name,
                icon: t.icon,
                sort: t.sort,
                food_count: None,
            }),
            is_marked_like: like,
            is_marked_not_recommend: not_rec,
            total_order_count: 0,
            completed_order_count: 0,
            last_order_time: None,
            last_complete_time: None,
            created_at: f.created_at,
            updated_at: f.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FoodWithStatsRecord {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub tag_id: Option<i64>,
    pub ingredients: Option<String>,
    pub steps: Option<String>,
    pub food_status: FoodStatusEnum,
    pub submit_role: SubmitRoleEnum,
    pub apply_status: ApplyStatusEnum,
    pub apply_remark: Option<String>,
    pub created_by: i64,
    pub owner_user_id: Option<i64>,
    pub group_id: Option<i64>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<i64>,
    pub is_del: i16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub total_order_count: Option<i32>,
    pub completed_order_count: Option<i32>,
    pub last_order_time: Option<DateTime<Utc>>,
    pub last_complete_time: Option<DateTime<Utc>>,
}

impl FoodOut {
    pub fn from_with_stats(
        row: FoodWithStatsRecord,
        tag: Option<TagRecord>,
        marks: Vec<MarkTypeEnum>,
    ) -> Self {
        let like = marks.iter().any(|m| matches!(m, MarkTypeEnum::LIKE));
        let not_rec = marks
            .iter()
            .any(|m| matches!(m, MarkTypeEnum::NotRecommend));
        FoodOut {
            food_id: row.food_id,
            food_name: row.food_name,
            food_photo: row.food_photo,
            ingredients: Vec::new(),
            steps: row.steps,
            food_status: row.food_status,
            apply_status: row.apply_status,
            submit_role: row.submit_role,
            apply_remark: row.apply_remark,
            tag: tag.map(|t| FoodTagOut {
                tag_id: t.tag_id,
                tag_name: t.tag_name,
                icon: t.icon,
                sort: t.sort,
                food_count: None,
            }),
            is_marked_like: like,
            is_marked_not_recommend: not_rec,
            total_order_count: row.total_order_count.unwrap_or(0),
            completed_order_count: row.completed_order_count.unwrap_or(0),
            last_order_time: row.last_order_time,
            last_complete_time: row.last_complete_time,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodCreateInput {
    pub food_name: String,
    pub food_photo: Option<String>,
    pub ingredients: Option<Vec<i64>>,
    pub steps: Option<String>,
    pub tag_id: Option<i64>,
    pub group_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodUpdateInput {
    pub food_id: i64,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
    pub ingredients: Option<Vec<i64>>,
    pub steps: Option<String>,
    pub tag_id: Option<i64>,
    pub apply_remark: Option<String>,
    pub food_status: Option<FoodStatusEnum>,
    pub apply_status: Option<ApplyStatusEnum>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct FoodFilterQuery {
    pub keyword: Option<String>,
    pub food_status: Option<FoodStatusEnum>,
    pub apply_status: Option<ApplyStatusEnum>,
    pub submit_role: Option<SubmitRoleEnum>,
    pub tag_id: Option<i64>,
    pub group_id: Option<i64>,
    pub only_active: Option<bool>,
    pub created_by: Option<i64>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodMarkActionInput {
    pub food_id: i64,
    pub mark_type: MarkTypeEnum,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxDrawInput {
    pub group_id: Option<i64>,
    pub tag_ids: Vec<i64>,
    pub limit_each: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxFoodSnapshot {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxDrawResultOut {
    pub results: Vec<BlindBoxFoodSnapshot>,
    pub requested_tags: Vec<i64>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct FoodCursor {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub food_id: i64,
}
