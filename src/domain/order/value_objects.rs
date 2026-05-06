// 领域层 - 订单值对象
// 包含订单状态枚举和状态转换逻辑

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::Type;

/// 订单状态枚举 - 映射到数据库 order_status_enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "order_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    /// 待接单
    #[serde(rename = "PENDING_ACCEPT")]
    PendingAccept,
    /// 进行中
    #[serde(rename = "IN_PROGRESS")]
    InProgress,
    /// 已拒绝
    #[serde(rename = "REJECTED")]
    Rejected,
    /// 接单人完成
    #[serde(rename = "BREEDER_FINISHED")]
    BreederFinished,
    /// 下单人关闭
    #[serde(rename = "BREEDER_CLOSED")]
    BreederClosed,
    /// 双方确认完成
    #[serde(rename = "CONFIRMED_FINISHED")]
    ConfirmedFinished,
    /// 双方确认未完成
    #[serde(rename = "CONFIRMED_UNFINISHED")]
    ConfirmedUnfinished,
    /// 超时
    #[serde(rename = "TIMEOUT")]
    Timeout,
    /// 已取消
    #[serde(rename = "CANCELLED")]
    Cancelled,
    /// 系统关闭
    #[serde(rename = "SYSTEM_CLOSED")]
    SystemClosed,
}

/// 订单状态转换规则
impl OrderStatus {
    /// 判断当前状态是否可以转换到目标状态
    pub fn can_transition(self, to: OrderStatus) -> bool {
        use OrderStatus::*;
        match (self, to) {
            (PendingAccept, InProgress | Rejected | Cancelled | Timeout | SystemClosed) => true,
            (InProgress, BreederFinished | BreederClosed | SystemClosed) => true,
            (BreederFinished, ConfirmedFinished | ConfirmedUnfinished | SystemClosed) => true,
            (Rejected, _) => false,
            (BreederClosed, _) => false,
            (ConfirmedFinished, _) => false,
            (ConfirmedUnfinished, BreederFinished | SystemClosed) => true,
            (Timeout, _) => false,
            (Cancelled, _) => false,
            (SystemClosed, _) => false,
            _ => false,
        }
    }
}
