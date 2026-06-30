// 领域层 - 心愿值对象
// 包含心愿状态枚举和状态转换逻辑
// FSD.latest.md compliant - 7状态模型

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::Type;

/// 心愿状态枚举 - 6状态模型(DRAFT 已删除,创建直接进入 NEGOTIATING)
/// 状态机:
///   NEGOTIATING → CREATED → CLAIMED → FINISHED
///                                     → EXPIRED
///   任意非终态 → CLOSED
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishStatus {
    /// 协商中 - 双方协商积分价格和履约期限
    #[serde(rename = "NEGOTIATING")]
    Negotiating,
    /// 已创建 - 双方已确认，进入组内心愿池
    #[serde(rename = "CREATED")]
    Created,
    /// 已选择 - 发起人已选择，积分已冻结，待履约
    #[serde(rename = "CLAIMED")]
    Claimed,
    /// 已完成 - 已履约并打卡，积分正式扣减
    #[serde(rename = "FINISHED")]
    Finished,
    /// 已逾期 - 履约人逾期未履约，积分已退还
    #[serde(rename = "EXPIRED")]
    Expired,
    /// 已关闭 - 双方关闭或作废
    #[serde(rename = "CLOSED")]
    Closed,
}

/// 心愿协商动作枚举
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_negotiation_action_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishNegotiationAction {
    /// 报价
    #[serde(rename = "QUOTE")]
    Quote,
    /// 还价
    #[serde(rename = "COUNTER")]
    Counter,
    /// 设置期限
    #[serde(rename = "SET_DEADLINE")]
    SetDeadline,
    /// 接受
    #[serde(rename = "ACCEPT")]
    Accept,
    /// 拒绝
    #[serde(rename = "REJECT")]
    Reject,
    /// 关闭
    #[serde(rename = "CLOSE")]
    Close,
}

/// 质量查看状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_quality_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishQualityStatus {
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "REVIEWED")]
    Reviewed,
}

/// 心愿状态转换规则 - FSD定义
#[allow(dead_code)]
impl WishStatus {
    /// 判断当前状态是否可以转换到目标状态
    pub fn can_transition(self, to: WishStatus) -> bool {
        use WishStatus::*;
        match (self, to) {
            // NEGOTIATING: 可进入CREATED(双方确认)或CLOSED
            (Negotiating, Created | Closed) => true,
            // CREATED: 可进入CLAIMED(选择)或CLOSED
            (Created, Claimed | Closed) => true,
            // CLAIMED: 可进入FINISHED(打卡)或EXPIRED(逾期)或CLOSED
            (Claimed, Finished | Expired | Closed) => true,
            // 终态不可转换
            (Finished, _) => false,
            (Expired, _) => false,
            (Closed, _) => false,
            _ => false,
        }
    }

    /// 判断是否为终态
    pub fn is_terminal(self) -> bool {
        matches!(self, WishStatus::Finished | WishStatus::Expired | WishStatus::Closed)
    }

    /// 判断是否为可选择状态(可被发起人选择并冻结积分)
    pub fn is_selectable(self) -> bool {
        matches!(self, WishStatus::Created)
    }

    /// 判断是否为可协商状态
    pub fn is_negotiable(self) -> bool {
        matches!(self, WishStatus::Negotiating)
    }
}
