// 领域层 - 事件类型定义
// 所有领域事件类型和 Payload 结构体

use serde::{Deserialize, Serialize};

// ============== 事件类型枚举 ==============

/// 事件类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventType {
    Unknown,           // 未知事件（用于错误处理）
    OrderCreated,      // 订单创建
    OrderAccepted,     // 订单被接受
    OrderCompleted,    // 订单完成
    OrderReviewed,     // 订单被评价
    FootprintPublished, // 足迹发布
    WishFulfilled,     // 心愿完成
    SignIn,            // 签到
    DiamondConsumed,   // 钻石消耗
    PointChanged,      // 积分变动
}

impl EventType {
    /// 获取事件类型的字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Unknown => "UnknownEvent",
            EventType::OrderCreated => "OrderCreatedEvent",
            EventType::OrderAccepted => "OrderAcceptedEvent",
            EventType::OrderCompleted => "OrderCompletedEvent",
            EventType::OrderReviewed => "OrderReviewedEvent",
            EventType::FootprintPublished => "FootprintPublishedEvent",
            EventType::WishFulfilled => "WishFulfilledEvent",
            EventType::SignIn => "SignInEvent",
            EventType::DiamondConsumed => "DiamondConsumedEvent",
            EventType::PointChanged => "PointChangedEvent",
        }
    }

    /// 从字符串解析事件类型
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "UnknownEvent" => Some(EventType::Unknown),
            "OrderCreatedEvent" => Some(EventType::OrderCreated),
            "OrderAcceptedEvent" => Some(EventType::OrderAccepted),
            "OrderCompletedEvent" => Some(EventType::OrderCompleted),
            "OrderReviewedEvent" => Some(EventType::OrderReviewed),
            "FootprintPublishedEvent" => Some(EventType::FootprintPublished),
            "WishFulfilledEvent" => Some(EventType::WishFulfilled),
            "SignInEvent" => Some(EventType::SignIn),
            "DiamondConsumedEvent" => Some(EventType::DiamondConsumed),
            "PointChangedEvent" => Some(EventType::PointChanged),
            _ => None,
        }
    }
}

// ============== 事件 Payload 定义 ==============

/// 订单创建事件 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderCreatedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub group_id: Option<i64>,
}

/// 订单被接受事件 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderAcceptedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub assignee_id: i64,
    pub group_id: Option<i64>,
}

/// 订单完成事件 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderCompletedPayload {
    pub order_id: i64,
    pub user_id: i64,
    pub assignee_id: i64,
    pub group_id: Option<i64>,
    pub points_earned: i32,
}

/// 订单评价事件 Payload
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintPublishedPayload {
    pub record_id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub order_id: Option<i64>,
}

/// 心愿完成事件 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishFulfilledPayload {
    pub wish_id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub points_spent: i32,
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
    pub diamonds_earned: i32,
}

/// 钻石消耗事件 Payload
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointChangedPayload {
    pub user_id: i64,
    pub amount: i32,
    pub tx_type: String,
    pub ref_id: Option<i64>,
}
