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
#[sqlx(type_name = "submit_role_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubmitRoleEnum {
    ORDERING_APPLY,
    RECEIVING_CREATE,
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
pub enum MarkTypeEnum {
    LIKE,
    NOT_RECOMMEND,
}

// food_types 数值型类别：1早餐 2午餐 3下午茶 4晚餐 5夜宵
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum FoodCategory {
    Breakfast = 1,
    Lunch = 2,
    AfternoonTea = 3,
    Dinner = 4,
    MidnightSnack = 5,
}
impl FoodCategory {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            1 => Some(Self::Breakfast),
            2 => Some(Self::Lunch),
            3 => Some(Self::AfternoonTea),
            4 => Some(Self::Dinner),
            5 => Some(Self::MidnightSnack),
            _ => None,
        }
    }
    pub fn zh_label(&self) -> &'static str {
        match self {
            Self::Breakfast => "早餐",
            Self::Lunch => "午餐",
            Self::AfternoonTea => "下午茶",
            Self::Dinner => "晚餐",
            Self::MidnightSnack => "夜宵",
        }
    }
}

// ================= Core DB Row Representations =================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct FoodRecord {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub food_types: i16, // 使用数值分类
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
pub struct TagRecord {
    pub tag_id: i64,
    pub tag_name: String,
    pub sort: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct FoodTagMapRecord {
    pub food_id: i64,
    pub tag_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodTagOut {
    pub tag_id: i64,
    pub tag_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodOut {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub category: FoodCategory,
    pub food_status: FoodStatusEnum,
    pub apply_status: ApplyStatusEnum,
    pub submit_role: SubmitRoleEnum,
    pub apply_remark: Option<String>,
    pub tags: Vec<FoodTagOut>,
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

impl From<(FoodRecord, Vec<TagRecord>, Vec<MarkTypeEnum>)> for FoodOut {
    fn from(value: (FoodRecord, Vec<TagRecord>, Vec<MarkTypeEnum>)) -> Self {
        let (f, tags, marks) = value;
        let like = marks.iter().any(|m| matches!(m, MarkTypeEnum::LIKE));
        let not_rec = marks
            .iter()
            .any(|m| matches!(m, MarkTypeEnum::NOT_RECOMMEND));
        Self {
            food_id: f.food_id,
            food_name: f.food_name,
            food_photo: f.food_photo,
            category: FoodCategory::from_i32(f.food_types as i32)
                .unwrap_or(FoodCategory::Breakfast),
            food_status: f.food_status,
            apply_status: f.apply_status,
            submit_role: f.submit_role,
            apply_remark: f.apply_remark,
            tags: tags
                .into_iter()
                .map(|t| FoodTagOut {
                    tag_id: t.tag_id,
                    tag_name: t.tag_name,
                })
                .collect(),
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
pub struct FoodWithStatsRecord {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub food_types: i16,
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
        tags: Vec<TagRecord>,
        marks: Vec<MarkTypeEnum>,
    ) -> Self {
        let like = marks.iter().any(|m| matches!(m, MarkTypeEnum::LIKE));
        let not_rec = marks.iter().any(|m| matches!(m, MarkTypeEnum::NOT_RECOMMEND));
        FoodOut {
            food_id: row.food_id,
            food_name: row.food_name,
            food_photo: row.food_photo,
            category: FoodCategory::from_i32(row.food_types as i32)
                .unwrap_or(FoodCategory::Breakfast),
            food_status: row.food_status,
            apply_status: row.apply_status,
            submit_role: row.submit_role,
            apply_remark: row.apply_remark,
            tags: tags
                .into_iter()
                .map(|t| FoodTagOut {
                    tag_id: t.tag_id,
                    tag_name: t.tag_name,
                })
                .collect(),
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
pub struct FoodCreateInput {
    pub food_name: String,
    pub food_photo: Option<String>,
    pub food_types: FoodCategory,
    pub tag_ids: Option<Vec<i64>>, // 关联标签
    pub group_id: Option<i64>,     // 归属组（可选）
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodUpdateInput {
    pub food_id: i64,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
    pub food_types: Option<FoodCategory>,
    pub tag_ids: Option<Vec<i64>>,
    pub apply_remark: Option<String>,
    pub food_status: Option<FoodStatusEnum>,
    pub apply_status: Option<ApplyStatusEnum>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TagCreateInput {
    pub tag_name: String,
    pub sort: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodFilterQuery {
    pub keyword: Option<String>,
    pub food_status: Option<FoodStatusEnum>,
    pub apply_status: Option<ApplyStatusEnum>,
    pub submit_role: Option<SubmitRoleEnum>,
    pub category: Option<FoodCategory>,
    pub tag_ids: Option<Vec<i64>>,
    pub group_id: Option<i64>,
    pub only_active: Option<bool>,
    pub created_by: Option<i64>,
}

// ================ 收藏/标记 DTOs ==================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodMarkActionInput {
    pub food_id: i64,
    pub mark_type: MarkTypeEnum,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodMarkOut {
    pub food_id: i64,
    pub mark_type: MarkTypeEnum,
    pub created_at: DateTime<Utc>,
}

// ================ Blind Box (抽取盲盒) ==================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BlindBoxDrawInput {
    pub group_id: Option<i64>,         // 若为空则按用户所属主 group
    pub food_types: Vec<FoodCategory>, // 需要抽取的类别集合
    pub limit_each: Option<u32>,       // 每个类型最多抽取数量
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BlindBoxFoodSnapshot {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub category: FoodCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BlindBoxDrawResultOut {
    pub results: Vec<BlindBoxFoodSnapshot>,
    pub requested_types: Vec<FoodCategory>,
}
