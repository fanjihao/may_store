use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ================= New Schema Enums (PostgreSQL) =================
// 为兼容新 schema_v1_pg.sql 中的枚举类型，添加对应 Rust 映射。
// 旧代码仍使用 i32 表示状态，后续可逐步迁移为这些强类型枚举。

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

// ================= Core DB Row Representations =================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodRecord {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    // food_types removed
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
}

// ================= Ingredients =================

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
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodOut {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    // category removed
    pub ingredients: Option<String>,
    pub steps: Option<String>,
    pub food_status: FoodStatusEnum,
    pub apply_status: ApplyStatusEnum,
    pub submit_role: SubmitRoleEnum,
    pub apply_remark: Option<String>,
    pub tag: Option<FoodTagOut>,
    pub is_marked_like: bool,
    pub is_marked_not_recommend: bool,
    // 统计字段（来自 food_stats 缓存表）
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
            ingredients: f.ingredients,
            steps: f.steps,
            food_status: f.food_status,
            apply_status: f.apply_status,
            submit_role: f.submit_role,
            apply_remark: f.apply_remark,
            tag: tag.map(|t| FoodTagOut {
                tag_id: t.tag_id,
                tag_name: t.tag_name,
                icon: t.icon,
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

// 专用于列表/详情的合并行（含统计）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FoodWithStatsRecord {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    // food_types removed
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
            ingredients: row.ingredients,
            steps: row.steps,
            food_status: row.food_status,
            apply_status: row.apply_status,
            submit_role: row.submit_role,
            apply_remark: row.apply_remark,
            tag: tag.map(|t| FoodTagOut {
                tag_id: t.tag_id,
                tag_name: t.tag_name,
                icon: t.icon,
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

// ================ Create / Update DTOs ==================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodCreateInput {
    pub food_name: String,
    pub food_photo: Option<String>,
    // food_types removed
    pub ingredients: Option<String>,
    pub steps: Option<String>,
    pub tag_id: Option<i64>,   // 关联标签
    pub group_id: Option<i64>, // 归属组（可选）
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodUpdateInput {
    pub food_id: i64,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
    // food_types removed
    pub ingredients: Option<String>,
    pub steps: Option<String>,
    pub tag_id: Option<i64>,
    pub apply_remark: Option<String>,
    pub food_status: Option<FoodStatusEnum>,
    pub apply_status: Option<ApplyStatusEnum>,
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

// ================ Ingredients DTOs ==================

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
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodFilterQuery {
    pub keyword: Option<String>,
    pub food_status: Option<FoodStatusEnum>,
    pub apply_status: Option<ApplyStatusEnum>,
    pub submit_role: Option<SubmitRoleEnum>,
    // category removed
    pub tag_id: Option<i64>,
    pub group_id: Option<i64>,
    pub only_active: Option<bool>,
    pub created_by: Option<i64>,
}

// ================ 收藏/标记 DTOs ==================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodMarkActionInput {
    pub food_id: i64,
    pub mark_type: MarkTypeEnum,
}

// ================ Blind Box (抽取盲盒) ==================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxDrawInput {
    pub group_id: Option<i64>,   // 若为空则按用户所属主 group
    pub tag_ids: Vec<i64>,       // 抽取的标签ID列表
    pub limit_each: Option<u32>, // 每个类型最多抽取数量
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxFoodSnapshot {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    // category removed
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlindBoxDrawResultOut {
    pub results: Vec<BlindBoxFoodSnapshot>,
    pub requested_tags: Vec<i64>,
}
