// 领域层 - 订单实体
// 包含订单记录结构和 DTO
// FSD.latest.md compliant

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::FromRow;

use super::{OrderStatus, OrderType, PointGrantStatus, ExpGrantStatus, RiskStatus};

/// 订单记录 - 从数据库查询得到的订单完整信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct OrderRecord {
    pub order_id: i64,
    pub user_id: i64,
    pub guest_id: Option<i64>,
    pub group_id: Option<i64>,
    pub status: OrderStatus,
    pub goal_time: Option<DateTime<Utc>>,
    pub remark: Option<String>,
    pub points_reward: i32,
    pub cancel_reason: Option<String>,
    pub reject_reason: Option<String>,
    pub last_status_change_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub is_guest: bool,
    // FSD v2 fields
    #[sqlx(default)]
    pub type_: Option<OrderType>,
    #[sqlx(default)]
    pub creator_role_snapshot: Option<String>,
    #[sqlx(default)]
    pub assignee_id: Option<i64>,
    #[sqlx(default)]
    pub assignee_role_snapshot: Option<String>,
    #[sqlx(default)]
    pub guest_user_id: Option<i64>,
    #[sqlx(default)]
    pub guest_invite_id: Option<i64>,
    #[sqlx(default)]
    pub guest_remark: Option<String>,
    #[sqlx(default)]
    pub guest_mark_tags: Option<serde_json::Value>,
    #[sqlx(default)]
    pub point_grant_status: Option<PointGrantStatus>,
    #[sqlx(default)]
    pub exp_grant_status: Option<ExpGrantStatus>,
    #[sqlx(default)]
    pub risk_status: Option<RiskStatus>,
    #[sqlx(default)]
    pub risk_detail: Option<serde_json::Value>,
    #[sqlx(default)]
    pub title: Option<String>,
    #[sqlx(default)]
    pub content: Option<String>,
    #[sqlx(default)]
    pub deadline: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub version: Option<i32>,
    #[sqlx(default)]
    pub accepted_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub confirmed_at: Option<DateTime<Utc>>,
}

/// 订单项记录
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct OrderItemRecord {
    pub id: i64,
    pub order_id: i64,
    pub food_id: i64,
    pub quantity: i32,
    pub price: Option<f64>,
    pub snapshot_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// 订单创建输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderCreateInput {
    pub group_id: Option<i64>,
    pub invite_code: Option<String>,
    pub goal_time: Option<DateTime<Utc>>,
    pub items: Vec<OrderItemCreateInput>,
    pub remark: Option<String>,
    pub points_reward: Option<i32>,
    pub is_guest: Option<bool>,
}

/// 订单项创建输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderItemCreateInput {
    pub food_id: i64,
    pub quantity: Option<i32>,
}

/// 订单状态更新输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderStatusUpdateInput {
    pub order_id: i64,
    pub to_status: OrderStatus,
    pub remark: Option<String>,
    pub points_reward: Option<i32>,
}

/// 订单查询参数
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct OrderQuery {
    pub user_id: Option<i64>,
    pub group_id: Option<i64>,
    pub status: Option<OrderStatus>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub expired_only: Option<bool>,
}

/// 订单统计
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderStatistics {
    pub pending_accept: i32,
    pub in_progress: i32,
    pub pending_confirm: i32,
}

/// 订单项输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderItemOut {
    pub id: i64,
    pub food_id: i64,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
    pub quantity: i32,
    pub price: Option<f64>,
}

/// 订单状态历史输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderStatusHistoryOut {
    pub from_status: Option<OrderStatus>,
    pub to_status: OrderStatus,
    pub changed_by: Option<String>,
    pub remark: Option<String>,
    pub changed_at: DateTime<Utc>,
}

/// 订单完整输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderOutNew {
    pub order_id: i64,
    pub user_id: i64,
    pub guest_id: Option<i64>,
    pub group_id: Option<i64>,
    pub status: OrderStatus,
    pub goal_time: Option<DateTime<Utc>>,
    pub remark: Option<String>,
    pub points_reward: i32,
    pub cancel_reason: Option<String>,
    pub reject_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_status_change_at: Option<DateTime<Utc>>,
    pub items: Vec<OrderItemOut>,
    pub status_history: Vec<OrderStatusHistoryOut>,
    pub is_guest: bool,
    pub group_name: Option<String>,
    pub group_info: Option<GroupInfoSimple>,
    pub receiver_nick_name: Option<String>,
    pub receiver_avatar: Option<String>,
}

/// 组简要信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfoSimple {
    pub group_id: i64,
    pub group_name: Option<String>,
}

impl From<OrderRecord> for OrderOutNew {
    fn from(r: OrderRecord) -> Self {
        Self {
            order_id: r.order_id,
            user_id: r.user_id,
            guest_id: r.guest_id,
            group_id: r.group_id,
            status: r.status,
            goal_time: r.goal_time,
            remark: r.remark,
            points_reward: r.points_reward,
            cancel_reason: r.cancel_reason,
            reject_reason: r.reject_reason,
            created_at: r.created_at,
            updated_at: r.updated_at,
            last_status_change_at: r.last_status_change_at,
            items: Vec::new(),
            status_history: Vec::new(),
            is_guest: r.is_guest,
            group_name: None,
            group_info: None,
            receiver_nick_name: None,
            receiver_avatar: None,
        }
    }
}

/// 订单评价创建输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderRatingCreateInput {
    pub order_id: i64,
    pub delta: i32,
    pub remark: Option<String>,
    pub target_user_id: Option<i64>,
}

/// 订单评价输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderRatingOut {
    pub rating_id: i64,
    pub order_id: i64,
    pub rater_user_id: i64,
    pub target_user_id: i64,
    pub delta: i32,
    pub remark: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 订单游标分页
#[derive(Debug, Deserialize, Serialize)]
pub struct OrderCursor {
    pub created_at: DateTime<Utc>,
    pub order_id: i64,
}

/// 团队今日订单查询
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct TeamTodayOrdersQuery {
    pub group_id: Option<i64>,
}
