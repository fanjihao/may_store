// 领域层 - 事件类型定义
// FSD.latest.md compliant - 完整的事件类型和Payload定义

use serde::{Deserialize, Serialize};

// ============== 事件类型枚举 ==============

/// 事件类型枚举 - FSD完整定义
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
    OrderRiskDetected,
    // 心愿事件
    WishCreated,
    WishNegotiating,
    WishAgreementConfirmed,
    WishSelected,
    WishFeedbackSubmitted,
    WishFinished,
    WishExpired,
    WishQualityRewarded,
    WishClosed,
    // 经济事件
    LovePointEarned,
    LovePointFrozen,
    LovePointUnfrozen,
    LovePointDeducted,
    GroupExpEarned,
    GroupLevelUp,
    DiamondEarned,
    // 组事件
    RoleSwapped,
    // 签到事件
    SignIn,
    // 兼容旧事件
    OrderReviewed,
    FootprintPublished,
    WishFulfilled,
    DiamondConsumed,
    PointChanged,
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
            EventType::OrderRiskDetected => "OrderRiskDetectedEvent",
            EventType::WishCreated => "WishCreatedEvent",
            EventType::WishNegotiating => "WishNegotiatingEvent",
            EventType::WishAgreementConfirmed => "WishAgreementConfirmedEvent",
            EventType::WishSelected => "WishSelectedEvent",
            EventType::WishFeedbackSubmitted => "WishFeedbackSubmittedEvent",
            EventType::WishFinished => "WishFinishedEvent",
            EventType::WishExpired => "WishExpiredEvent",
            EventType::WishQualityRewarded => "WishQualityRewardedEvent",
            EventType::WishClosed => "WishClosedEvent",
            EventType::LovePointEarned => "LovePointEarnedEvent",
            EventType::LovePointFrozen => "LovePointFrozenEvent",
            EventType::LovePointUnfrozen => "LovePointUnfrozenEvent",
            EventType::LovePointDeducted => "LovePointDeductedEvent",
            EventType::GroupExpEarned => "GroupExpEarnedEvent",
            EventType::GroupLevelUp => "GroupLevelUpEvent",
            EventType::DiamondEarned => "DiamondEarnedEvent",
            EventType::RoleSwapped => "RoleSwappedEvent",
            EventType::SignIn => "SignInEvent",
            EventType::OrderReviewed => "OrderReviewedEvent",
            EventType::FootprintPublished => "FootprintPublishedEvent",
            EventType::WishFulfilled => "WishFulfilledEvent",
            EventType::DiamondConsumed => "DiamondConsumedEvent",
            EventType::PointChanged => "PointChangedEvent",
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
            "OrderRiskDetectedEvent" => Some(EventType::OrderRiskDetected),
            "WishCreatedEvent" => Some(EventType::WishCreated),
            "WishNegotiatingEvent" => Some(EventType::WishNegotiating),
            "WishAgreementConfirmedEvent" => Some(EventType::WishAgreementConfirmed),
            "WishSelectedEvent" => Some(EventType::WishSelected),
            "WishFeedbackSubmittedEvent" => Some(EventType::WishFeedbackSubmitted),
            "WishFinishedEvent" => Some(EventType::WishFinished),
            "WishExpiredEvent" => Some(EventType::WishExpired),
            "WishQualityRewardedEvent" => Some(EventType::WishQualityRewarded),
            "WishClosedEvent" => Some(EventType::WishClosed),
            "LovePointEarnedEvent" => Some(EventType::LovePointEarned),
            "LovePointFrozenEvent" => Some(EventType::LovePointFrozen),
            "LovePointUnfrozenEvent" => Some(EventType::LovePointUnfrozen),
            "LovePointDeductedEvent" => Some(EventType::LovePointDeducted),
            "GroupExpEarnedEvent" => Some(EventType::GroupExpEarned),
            "GroupLevelUpEvent" => Some(EventType::GroupLevelUp),
            "DiamondEarnedEvent" => Some(EventType::DiamondEarned),
            "RoleSwappedEvent" => Some(EventType::RoleSwapped),
            "SignInEvent" => Some(EventType::SignIn),
            "OrderReviewedEvent" => Some(EventType::OrderReviewed),
            "FootprintPublishedEvent" => Some(EventType::FootprintPublished),
            "WishFulfilledEvent" => Some(EventType::WishFulfilled),
            "DiamondConsumedEvent" => Some(EventType::DiamondConsumed),
            "PointChangedEvent" => Some(EventType::PointChanged),
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

/// 订单完成事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderCompletedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub assignee_id: i64,
    pub group_id: Option<i64>,
    pub order_type: String,
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

/// 订单风控检测事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRiskDetectedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub risk_status: String,
    pub risk_detail: Option<serde_json::Value>,
    pub trace_id: Option<String>,
}

/// 心愿创建事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishCreatedPayload {
    pub wish_id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub initial_cost: i32,
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

/// 心愿打卡提交事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishFeedbackSubmittedPayload {
    pub wish_id: i64,
    pub requester_id: i64,
    pub fulfiller_id: i64,
    pub group_id: i64,
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

/// 心愿质量奖励事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishQualityRewardedPayload {
    pub wish_id: i64,
    pub group_id: i64,
    pub reviewer_id: i64,
    pub quality_level: String,
    pub diamond_reward: i32,
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

/// 爱心积分获得事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LovePointEarnedPayload {
    pub user_id: i64,
    pub group_id: i64,
    pub amount: i32,
    pub biz_type: String,
    pub biz_id: i64,
    pub trace_id: Option<String>,
}

/// 爱心积分冻结事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LovePointFrozenPayload {
    pub user_id: i64,
    pub group_id: i64,
    pub amount: i32,
    pub biz_type: String,
    pub biz_id: i64,
    pub trace_id: Option<String>,
}

/// 爱心积分解冻事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LovePointUnfrozenPayload {
    pub user_id: i64,
    pub group_id: i64,
    pub amount: i32,
    pub biz_type: String,
    pub biz_id: i64,
    pub trace_id: Option<String>,
}

/// 爱心积分扣减事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LovePointDeductedPayload {
    pub user_id: i64,
    pub group_id: i64,
    pub amount: i32,
    pub biz_type: String,
    pub biz_id: i64,
    pub trace_id: Option<String>,
}

/// 组经验获得事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupExpEarnedPayload {
    pub group_id: i64,
    pub amount: i32,
    pub exp_before: i64,
    pub exp_after: i64,
    pub biz_type: String,
    pub biz_id: i64,
    pub trace_id: Option<String>,
}

/// 组等级提升事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupLevelUpPayload {
    pub group_id: i64,
    pub old_level: i32,
    pub new_level: i32,
    pub trace_id: Option<String>,
}

/// 组钻石获得事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiamondEarnedPayload {
    pub group_id: i64,
    pub amount: i32,
    pub biz_type: String,
    pub biz_id: i64,
    pub trace_id: Option<String>,
}

/// 角色互换事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleSwappedPayload {
    pub group_id: i64,
    pub user_id: i64,
    pub old_buyer_id: Option<i64>,
    pub old_seller_id: Option<i64>,
    pub new_buyer_id: Option<i64>,
    pub new_seller_id: Option<i64>,
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

// ============== 兼容旧事件Payload ==============

/// 订单评价事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderReviewedPayload {
    pub order_id: i64,
    pub rater_user_id: i64,
    pub target_user_id: i64,
    pub rating_delta: i32,
    pub group_id: Option<i64>,
}

/// 足迹发布事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintPublishedPayload {
    pub record_id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub order_id: Option<i64>,
}

/// 心愿完成事件 Payload (旧版兼容)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishFulfilledPayload {
    pub wish_id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub points_spent: i32,
}

/// 钻石消耗事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiamondConsumedPayload {
    pub user_id: i64,
    pub group_id: Option<i64>,
    pub amount: i32,
    pub scene: String,
    pub relation_id: Option<i64>,
}

/// 积分变动事件 Payload
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointChangedPayload {
    pub user_id: i64,
    pub amount: i32,
    pub tx_type: String,
    pub ref_id: Option<i64>,
}
