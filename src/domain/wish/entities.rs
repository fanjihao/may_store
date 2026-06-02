// 领域层 - 心愿实体
// 包含心愿记录、反馈等数据库记录和 DTO
// FSD.latest.md compliant - 7状态模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::FromRow;
use utoipa::ToSchema;

use super::{WishStatus, WishQualityStatus};

/// 心愿记录 - FSD v2版本
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishRecord {
    pub wish_id: i64,
    pub wish_name: String,
    pub wish_cost: i32,
    pub status: WishStatus,
    pub created_by: i64,
    pub group_id: i64,
    pub claimed_by: Option<i64>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_cost: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // FSD v2 fields
    #[sqlx(default)]
    pub requester_id: Option<i64>,
    #[sqlx(default)]
    pub fulfiller_id: Option<i64>,
    #[sqlx(default)]
    pub creator_role_snapshot: Option<String>,
    #[sqlx(default)]
    pub initial_cost: Option<i32>,
    #[sqlx(default)]
    pub final_cost: Option<i32>,
    #[sqlx(default)]
    pub fulfillment_deadline_hours: Option<i32>,
    #[sqlx(default)]
    pub selected_by: Option<i64>,
    #[sqlx(default)]
    pub selected_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub fulfillment_due_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub fulfilled_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub expired_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub quality_review_status: Option<WishQualityStatus>,
    #[sqlx(default)]
    pub quality_reviewer_id: Option<i64>,
    #[sqlx(default)]
    pub quality_remark: Option<String>,
    #[sqlx(default)]
    pub diamond_reward: Option<i32>,
    #[sqlx(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub closed_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub version: Option<i32>,
}

/// 心愿协商记录
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct WishNegotiationRecord {
    pub id: i64,
    pub wish_id: i64,
    pub group_id: i64,
    pub operator_id: i64,
    pub operator_role_snapshot: Option<String>,
    pub action: String,
    pub cost: Option<i32>,
    pub deadline_hours: Option<i32>,
    pub remark: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 心愿打卡记录
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct WishCheckinRecord {
    pub id: i64,
    pub wish_id: i64,
    pub user_id: i64,
    pub content: Option<String>,
    pub location: Option<String>,
    pub images: Option<Json<Vec<String>>>,
    pub created_at: DateTime<Utc>,
}

/// 心愿反馈记录（数据库记录格式）
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct WishFeedbackRecord {
    pub feedback_id: i64,
    pub wish_id: i64,
    pub user_id: i64,
    pub content: Option<String>,
    pub images: Option<Json<Vec<String>>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 心愿反馈输出（API 格式）
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishFeedbackOut {
    pub feedback_id: i64,
    pub user_id: i64,
    pub content: Option<String>,
    pub images: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<WishFeedbackRecord> for WishFeedbackOut {
    fn from(r: WishFeedbackRecord) -> Self {
        Self {
            feedback_id: r.feedback_id,
            user_id: r.user_id,
            content: r.content,
            images: r.images.map(|j| j.0),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// 心愿创建输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishCreateInput {
    pub wish_name: String,
    pub wish_cost: i32,
    pub group_id: i64,
}

/// 心愿更新输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishUpdateInput {
    pub wish_name: Option<String>,
    pub wish_cost: Option<i32>,
    pub status: Option<WishStatus>,
}

/// 心愿反馈输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishFeedbackInput {
    pub content: Option<String>,
    pub images: Option<Vec<String>>,
}

/// 心愿报价输入 (FSD v2)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishQuoteInput {
    pub cost: i32,
}

/// 心愿设置履约期限输入 (FSD v2)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishDeadlineInput {
    pub deadline_hours: i32,
}

/// 心愿拒绝/关闭输入 (FSD v2)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishRejectInput {
    pub reason: Option<String>,
}

/// 心愿输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishOut {
    pub wish_id: i64,
    pub wish_name: String,
    pub wish_cost: i32,
    pub status: WishStatus,
    pub created_by: i64,
    pub group_id: i64,
    pub claimed_by: Option<i64>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_cost: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub feedback: Option<WishFeedbackOut>,
}

impl WishOut {
    pub fn from_record(r: WishRecord, f: Option<WishFeedbackRecord>) -> Self {
        Self {
            wish_id: r.wish_id,
            wish_name: r.wish_name,
            wish_cost: r.wish_cost,
            status: r.status,
            created_by: r.created_by,
            group_id: r.group_id,
            claimed_by: r.claimed_by,
            claimed_at: r.claimed_at,
            claim_cost: r.claim_cost,
            created_at: r.created_at,
            updated_at: r.updated_at,
            feedback: f.map(WishFeedbackOut::from),
        }
    }
}

/// 心愿查询参数
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct WishQuery {
    pub group_id: Option<i64>,
    pub status: Option<WishStatus>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

/// 心愿游标分页
#[derive(Debug, Deserialize, Serialize)]
pub struct WishCursor {
    pub created_at: DateTime<Utc>,
    pub wish_id: i64,
}
