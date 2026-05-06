// 应用服务层 - 通知服务
// 包含微信推送等通知业务用例

use sqlx::PgPool;
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;

/// 通知类型
#[derive(Debug, Clone, Copy)]
pub enum NotificationType {
    OrderCreated,
    OrderAccepted,
    OrderCompleted,
    WishFulfilled,
    SignIn,
}

/// 通知应用服务
pub struct NotificationService;

impl NotificationService {
    /// 发送订单通知
    pub async fn push_order_notification(
        state: &Arc<AppState>,
        order_id: i64,
        notification_type: NotificationType,
    ) -> Result<(), CustomError> {
        // TODO: 迁移自 services/notifications.rs
        todo!("迁移通知推送逻辑")
    }
}
