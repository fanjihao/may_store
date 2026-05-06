// 应用服务层 - 签到事件处理器
// 处理 SignInEvent，发放签到奖励

use sqlx::PgPool;

use crate::domain::event::types::SignInPayload;
use crate::errors::CustomError;

/// 处理签到事件
pub async fn handle_sign_in(
    db: &PgPool,
    payload: &SignInPayload,
) -> Result<(), CustomError> {
    // TODO: 迁移自 worker/handlers/sign.rs
    // 1. 检查是否已经处理过（幂等）
    // 2. 发放签到奖励钻石
    todo!("迁移签到事件处理逻辑")
}
