// 领域层 - 足迹实体
// 包含足迹记录、足迹组等数据库记录和 DTO

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 足迹组记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordGroup {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub group_type: i16,
    pub max_capacity: i32,
    pub current_count: i32,
    pub status: i16,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

/// 用户足迹记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintRecord {
    pub id: i64,
    pub group_id: i64,
    pub record_group_id: i64,
    pub user_id: i64,
    pub order_id: Option<i64>,
    pub title: Option<String>,
    pub images: String,
    pub content: Option<String>,
    pub address: Option<String>,
    pub record_time: DateTime<Utc>,
    pub like_count: i32,
    pub comment_count: i32,
    pub is_draft: i16,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

/// 足迹概览统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintOverview {
    pub together_days: i32,
    pub total_feedings: i32,
    pub streak_days: i32,
    pub total_records: i32,
    pub streak_progress: f32,
    pub feeding_text: String,
    pub diamond_balance: i32,
    pub footprint_capacity: i32,
    pub footprint_count: i32,
}

/// 足迹创建输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordCreateInput {
    pub record_group_id: i64,
    pub title: Option<String>,
    pub images: Vec<String>,
    pub content: Option<String>,
    pub address: Option<String>,
    pub record_time: Option<String>,
}

/// 足迹更新输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordUpdateInput {
    pub title: Option<String>,
    pub images: Option<Vec<String>>,
    pub content: Option<String>,
    pub address: Option<String>,
    pub record_time: Option<String>,
    pub record_group_id: Option<i64>,
}

/// 草稿确认输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DraftConfirmInput {
    pub draft_id: i64,
    pub is_edit: bool,
    pub content: Option<String>,
    pub images: Option<Vec<String>>,
}

/// 容量扩展输入
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapacityExpandInput {
    pub record_group_id: i64,
    pub expand_level: i16,
}

/// 足迹游标分页
#[derive(Debug, Deserialize, Serialize)]
pub struct RecordCursor {
    pub record_time: DateTime<Utc>,
    pub id: i64,
}

/// 足迹查询参数
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct RecordQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub record_group_id: Option<i64>,
}

/// 足迹输出
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordOut {
    #[serde(flatten)]
    pub base: FootprintRecord,
    pub user_nick_name: Option<String>,
    pub user_avatar: Option<String>,
    pub is_liked: bool,
}

/// 权限检查响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckPermissionResponse {
    pub has_group: bool,
    pub group_id: Option<i64>,
}

/// 提交足迹响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitRecordResponse {
    pub record_id: i64,
}
