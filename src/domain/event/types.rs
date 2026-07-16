// 领域层 - 事件类型定义
// FSD.latest.md compliant - 完整的事件类型和Payload定义

use serde::{Deserialize, Serialize};

// ============== 事件类型枚举 ==============

/// 事件类型枚举 - 只保留实际有 publish 调用的变体
///
/// 2026-07-15 P1-1 清理:删除了 17 个死代码变体 (OrderRiskDetected / WishCreated /
/// WishFeedbackSubmitted / WishQualityRewarded / LovePointEarned~Deducted /
/// GroupExpEarned / GroupLevelUp / DiamondEarned / RoleSwapped / OrderReviewed /
/// FootprintPublished / WishFulfilled / DiamondConsumed / PointChanged),
/// 它们只在自己文件的 as_str/from_str 出现,0 publish 0 订阅
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventType {
    Unknown,
    // 订单事件
    OrderCreated,
    OrderAccepted,
    OrderCompleted,
    OrderConfirmedCompleted,
    OrderConfirmedIncomplete,
    OrderCancelled,
    OrderRejected,
    OrderTimeout,
    // 心愿事件
    WishNegotiating,
    WishAgreementConfirmed,
    WishSelected,
    WishFinished,
    WishExpired,
    WishClosed,
    // 签到事件
    SignIn,
}

impl EventType {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Unknown => "UnknownEvent",
            EventType::OrderCreated => "OrderCreatedEvent",
            EventType::OrderAccepted => "OrderAcceptedEvent",
            EventType::OrderCompleted => "OrderCompletedEvent",
            EventType::OrderConfirmedCompleted => "OrderConfirmedCompletedEvent",
            EventType::OrderConfirmedIncomplete => "OrderConfirmedIncompleteEvent",
            EventType::OrderCancelled => "OrderCancelledEvent",
            EventType::OrderRejected => "OrderRejectedEvent",
            EventType::OrderTimeout => "OrderTimeoutEvent",
            EventType::WishNegotiating => "WishNegotiatingEvent",
            EventType::WishAgreementConfirmed => "WishAgreementConfirmedEvent",
            EventType::WishSelected => "WishSelectedEvent",
            EventType::WishFinished => "WishFinishedEvent",
            EventType::WishExpired => "WishExpiredEvent",
            EventType::WishClosed => "WishClosedEvent",
            EventType::SignIn => "SignInEvent",
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "UnknownEvent" => Some(EventType::Unknown),
            "OrderCreatedEvent" => Some(EventType::OrderCreated),
            "OrderAcceptedEvent" => Some(EventType::OrderAccepted),
            "OrderCompletedEvent" => Some(EventType::OrderCompleted),
            "OrderConfirmedCompletedEvent" => Some(EventType::OrderConfirmedCompleted),
            "OrderConfirmedIncompleteEvent" => Some(EventType::OrderConfirmedIncomplete),
            "OrderCancelledEvent" => Some(EventType::OrderCancelled),
            "OrderRejectedEvent" => Some(EventType::OrderRejected),
            "OrderTimeoutEvent" => Some(EventType::OrderTimeout),
            "WishNegotiatingEvent" => Some(EventType::WishNegotiating),
            "WishAgreementConfirmedEvent" => Some(EventType::WishAgreementConfirmed),
            "WishSelectedEvent" => Some(EventType::WishSelected),
            "WishFinishedEvent" => Some(EventType::WishFinished),
            "WishExpiredEvent" => Some(EventType::WishExpired),
            "WishClosedEvent" => Some(EventType::WishClosed),
            "SignInEvent" => Some(EventType::SignIn),
            _ => None,
        }
    }
}

// ============== FSD事件Payload定义 ==============

/// 订单创建事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderCreatedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub group_id: Option<i64>,
    pub order_type: String, // NORMAL or GUEST
    pub trace_id: Option<String>,
}

/// 订单被接受事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderAcceptedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub assignee_id: i64,
    pub group_id: Option<i64>,
    pub trace_id: Option<String>,
}

/// 订单确认完成事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderConfirmedCompletedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub assignee_id: i64,
    pub group_id: i64,
    pub love_point_reward: i32,
    pub group_exp_reward: i32,
    pub trace_id: Option<String>,
}

/// 订单确认未完成事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderConfirmedIncompletePayload {
    pub order_id: i64,
    pub user_id: i64,
    pub assignee_id: i64,
    pub group_id: i64,
    pub trace_id: Option<String>,
}

/// 心愿协商事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishNegotiatingPayload {
    pub wish_id: i64,
    pub operator_id: i64,
    pub group_id: i64,
    pub action: String,
    pub cost: Option<i32>,
    pub deadline_hours: Option<i32>,
    pub trace_id: Option<String>,
}

/// 心愿双方确认事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishAgreementConfirmedPayload {
    pub wish_id: i64,
    pub requester_id: i64,
    pub fulfiller_id: i64,
    pub group_id: i64,
    pub final_cost: i32,
    pub fulfillment_deadline_hours: i32,
    pub trace_id: Option<String>,
}

/// 心愿选择事件 Payload - 发起人选择心愿并冻结积分
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishSelectedPayload {
    pub wish_id: i64,
    pub requester_id: i64,
    pub fulfiller_id: i64,
    pub group_id: i64,
    pub frozen_amount: i32,
    pub fulfillment_due_at: String,
    pub trace_id: Option<String>,
}

/// 心愿完成事件 Payload - 打卡完成，积分正式扣减
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishFinishedPayload {
    pub wish_id: i64,
    pub requester_id: i64,
    pub fulfiller_id: i64,
    pub group_id: i64,
    pub deducted_amount: i32,
    pub trace_id: Option<String>,
}

/// 心愿逾期事件 Payload - 履约人逾期，积分退还
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishExpiredPayload {
    pub wish_id: i64,
    pub requester_id: i64,
    pub fulfiller_id: i64,
    pub group_id: i64,
    pub unfrozen_amount: i32,
    pub trace_id: Option<String>,
}

/// 心愿关闭事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishClosedPayload {
    pub wish_id: i64,
    pub operator_id: i64,
    pub group_id: i64,
    pub reason: Option<String>,
    pub unfrozen_if_any: bool,
    pub trace_id: Option<String>,
}

/// 签到事件 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInPayload {
    pub sign_id: i64,
    pub user_id: i64,
    pub group_id: Option<i64>,
    pub sign_date: String,
    pub consecutive_days: i32,
    pub diamond_reward: i32,
    pub trace_id: Option<String>,
}
