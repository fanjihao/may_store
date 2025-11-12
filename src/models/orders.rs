// ================= New Order Models (Refactored to new PostgreSQL schema) =================
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// Strongly typed status enum mapping to order_status_enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "order_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatusEnum {
    PENDING,
    ACCEPTED,
    FINISHED,
    CANCELLED,
    EXPIRED,
    REJECTED,
    SYSTEM_CLOSED,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct OrderRecord {
    pub order_id: i64,
    pub user_id: i64,
    pub receiver_id: Option<i64>,
    pub group_id: Option<i64>,
    pub status: OrderStatusEnum,
    pub goal_time: Option<DateTime<Utc>>,
    pub points_cost: i32,
    pub points_reward: i32,
    pub cancel_reason: Option<String>,
    pub reject_reason: Option<String>,
    pub last_status_change_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct OrderItemRecord {
    pub id: i64,
    pub order_id: i64,
    pub food_id: i64,
    pub quantity: i32,
    pub price: Option<f64>,
    pub snapshot_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct OrderStatusHistoryRecord {
    pub id: i64,
    pub order_id: i64,
    pub from_status: Option<OrderStatusEnum>,
    pub to_status: OrderStatusEnum,
    pub changed_by: Option<i64>,
    pub remark: Option<String>,
    pub changed_at: DateTime<Utc>,
}

// ================== Public API DTOs ==================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderItemCreateInput {
    pub food_id: i64,
    pub quantity: Option<i32>, // default 1
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderCreateInput {
    pub receiver_id: Option<i64>,
    pub group_id: Option<i64>,
    pub goal_time: Option<DateTime<Utc>>,
    pub items: Vec<OrderItemCreateInput>,
    pub points_cost: Option<i32>,
    pub points_reward: Option<i32>, // 预设奖励（可由系统校验/忽略）
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderStatusUpdateInput {
    pub order_id: i64,
    pub to_status: OrderStatusEnum,
    pub remark: Option<String>,
    pub points_reward: Option<i32>, // 完成时奖励覆盖
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct OrderQuery {
    pub user_id: Option<i64>,     // 下单人过滤
    pub receiver_id: Option<i64>, // 接单人过滤
    pub status: Option<OrderStatusEnum>,
    pub limit: Option<i64>,
    /// 仅返回已经失效(状态=EXPIRED， CANCELLED， REJECTED， SYSTEM_CLOSED)的订单；与 status 同时出现时优先 status
    pub expired_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderItemOut {
    pub id: i64,
    pub food_id: i64,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
    pub quantity: i32,
    pub price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderStatusHistoryOut {
    pub from_status: Option<OrderStatusEnum>,
    pub to_status: OrderStatusEnum,
    pub changed_by: Option<i64>,
    pub remark: Option<String>,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderOutNew {
    pub order_id: i64,
    pub user_id: i64,
    pub receiver_id: Option<i64>,
    pub group_id: Option<i64>,
    pub status: OrderStatusEnum,
    pub goal_time: Option<DateTime<Utc>>,
    pub points_cost: i32,
    pub points_reward: i32,
    pub cancel_reason: Option<String>,
    pub reject_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_status_change_at: Option<DateTime<Utc>>,
    pub items: Vec<OrderItemOut>,
    pub status_history: Vec<OrderStatusHistoryOut>,
}

impl From<(OrderRecord, Vec<OrderItemOut>, Vec<OrderStatusHistoryOut>)> for OrderOutNew {
    fn from(value: (OrderRecord, Vec<OrderItemOut>, Vec<OrderStatusHistoryOut>)) -> Self {
        let (r, items, history) = value;
        Self {
            order_id: r.order_id,
            user_id: r.user_id,
            receiver_id: r.receiver_id,
            group_id: r.group_id,
            status: r.status,
            goal_time: r.goal_time,
            points_cost: r.points_cost,
            points_reward: r.points_reward,
            cancel_reason: r.cancel_reason,
            reject_reason: r.reject_reason,
            created_at: r.created_at,
            updated_at: r.updated_at,
            last_status_change_at: r.last_status_change_at,
            items,
            status_history: history,
        }
    }
}

// ================= Ratings (Post-Finish +/- points) =================
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderRatingCreateInput {
    pub order_id: i64,
    pub delta: i32, // -5..5 (非0)
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderRatingOut {
    pub rating_id: i64,
    pub order_id: i64,
    pub rater_user_id: i64,
    pub target_user_id: i64,
    pub delta: i32,
    pub remark: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ================= Transition Helpers =================
impl OrderStatusEnum {
    pub fn can_transition(self, to: OrderStatusEnum) -> bool {
        use OrderStatusEnum::*;
        match (self, to) {
            (PENDING, ACCEPTED | REJECTED | CANCELLED | EXPIRED) => true,
            (ACCEPTED, FINISHED | CANCELLED | REJECTED) => true,
            (REJECTED, _) => false,
            (FINISHED, _) => false,
            (CANCELLED, _) => false,
            (EXPIRED, _) => false,
            (SYSTEM_CLOSED, _) => false,
            _ => false,
        }
    }
}

// ================= Deprecated (Removed Old Structures) =================
// 原有旧结构全部移除，若需要兼容可在此添加 #[deprecated] stub。
