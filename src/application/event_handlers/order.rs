// 应用服务层 - 订单事件处理器
// 处理订单相关事件

use sqlx::PgPool;

use crate::domain::event::types::{OrderCompletedPayload, OrderCreatedPayload};
use crate::errors::CustomError;

/// 处理订单创建事件
pub async fn handle_order_created(
    db: &PgPool,
    payload: &OrderCreatedPayload,
) -> Result<(), CustomError> {
    // TODO: 迁移自 worker/handlers/order.rs
    todo!("迁移订单创建事件处理逻辑")
}

/// 处理订单完成事件
pub async fn handle_order_completed(
    db: &PgPool,
    payload: &OrderCompletedPayload,
) -> Result<(), CustomError> {
    // TODO: 迁移自 worker/handlers/order.rs
    // 1. 发放积分奖励
    // 2. 如果评价好，发放钻石奖励
    // 3. 发布足迹
    // 4. 检查成就
    todo!("迁移订单完成事件处理逻辑")
}
