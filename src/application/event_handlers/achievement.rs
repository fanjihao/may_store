// 应用服务层 - 成就事件处理器
// 检查并解锁用户成就

use sqlx::PgPool;

use crate::domain::event::EventType;
use crate::errors::CustomError;

/// 检查并更新用户成就
pub async fn check_achievements(
    db: &PgPool,
    user_id: i64,
    event_type: EventType,
) -> Result<(), CustomError> {
    // TODO: 迁移自 worker/handlers/achievement.rs
    // 根据事件类型检查对应的成就规则
    // 如果满足条件，解锁成就并发送通知
    todo!("迁移成就检查逻辑")
}
