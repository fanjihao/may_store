// 领域层 - 订单值对象
// 包含订单状态枚举和状态转换逻辑
// FSD.latest.md compliant - 6核心状态 + 3终态

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::Type;

/// 订单状态枚举 - FSD定义6核心状态 + 2终态(REJECTED/CANCELLED) + TIMEOUT
/// 状态机:
///   CREATED → ACCEPTED → PRODUCTION_COMPLETED → CONFIRMED_COMPLETED
///                                                → CONFIRMED_INCOMPLETE
///   CREATED → REJECTED / CANCELLED
///   CREATED/ACCEPTED → TIMEOUT
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "order_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    /// 待接单 - 订单创建后默认状态
    #[serde(rename = "CREATED")]
    Created,
    /// 已接单 - Seller接受订单
    #[serde(rename = "ACCEPTED")]
    Accepted,
    /// 生产完成 - Seller完成制作/履约
    #[serde(rename = "PRODUCTION_COMPLETED")]
    ProductionCompleted,
    /// 双方确认完成 - Buyer确认履约质量
    #[serde(rename = "CONFIRMED_COMPLETED")]
    ConfirmedCompleted,
    /// 双方确认未完成 - Buyer确认未达标准
    #[serde(rename = "CONFIRMED_INCOMPLETE")]
    ConfirmedIncomplete,
    /// 已拒绝 - Seller拒绝接单
    #[serde(rename = "REJECTED")]
    Rejected,
    /// 已取消 - 订单被取消
    #[serde(rename = "CANCELLED")]
    Cancelled,
    /// 超时 - 订单超时未处理
    #[serde(rename = "TIMEOUT")]
    Timeout,
}

/// 订单类型枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "order_type_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderType {
    /// 组内普通订单
    #[serde(rename = "NORMAL")]
    Normal,
    /// 做客订单 - 受邀好友访问主人家厨房后创建
    #[serde(rename = "GUEST")]
    Guest,
}

/// 积分发放状态枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "point_grant_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PointGrantStatus {
    /// 无积分发放
    #[serde(rename = "NONE")]
    None,
    /// 待人工审核 - 风控判定需审核
    #[serde(rename = "PENDING_REVIEW")]
    PendingReview,
    /// 已发放
    #[serde(rename = "GRANTED")]
    Granted,
    /// 已拒绝
    #[serde(rename = "REJECTED")]
    Rejected,
    /// 已撤销 - 事后风控判定
    #[serde(rename = "REVOKED")]
    Revoked,
    /// 超过每日上限被拒绝
    #[serde(rename = "REJECTED_LIMIT")]
    RejectedLimit,
}

/// 经验发放状态枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "exp_grant_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpGrantStatus {
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "PENDING_REVIEW")]
    PendingReview,
    #[serde(rename = "GRANTED")]
    Granted,
    #[serde(rename = "REJECTED")]
    Rejected,
    #[serde(rename = "REVOKED")]
    Revoked,
    #[serde(rename = "REJECTED_LIMIT")]
    RejectedLimit,
}

/// 风控状态枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "risk_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskStatus {
    /// 通过
    #[serde(rename = "PASS")]
    Pass,
    /// 可疑 - 需要人工审核
    #[serde(rename = "SUSPECT")]
    Suspect,
    /// 拦截 - 直接拒绝
    #[serde(rename = "BLOCKED")]
    Blocked,
}

/// 订单状态转换规则 - FSD定义
#[allow(dead_code)]
impl OrderStatus {
    /// 判断当前状态是否可以转换到目标状态
    pub fn can_transition(self, to: OrderStatus) -> bool {
        use OrderStatus::*;
        match (self, to) {
            // CREATED: 可进入ACCEPTED/REJECTED/CANCELLED/TIMEOUT
            (Created, Accepted | Rejected | Cancelled | Timeout) => true,
            // ACCEPTED: 可进入PRODUCTION_COMPLETED/TIMEOUT
            (Accepted, ProductionCompleted | Timeout) => true,
            // PRODUCTION_COMPLETED: 可进入CONFIRMED_COMPLETED/CONFIRMED_INCOMPLETE
            (ProductionCompleted, ConfirmedCompleted | ConfirmedIncomplete) => true,
            // 终态不可转换
            (ConfirmedCompleted, _) => false,
            (ConfirmedIncomplete, _) => false,
            (Rejected, _) => false,
            (Cancelled, _) => false,
            (Timeout, _) => false,
            _ => false,
        }
    }

    /// 判断是否为终态
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            OrderStatus::ConfirmedCompleted
                | OrderStatus::ConfirmedIncomplete
                | OrderStatus::Rejected
                | OrderStatus::Cancelled
                | OrderStatus::Timeout
        )
    }
}
