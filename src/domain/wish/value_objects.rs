// 领域层 - 心愿值对象
// 包含心愿状态枚举

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::Type;

/// 心愿状态枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type, PartialEq, Eq)]
#[sqlx(type_name = "wish_status_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WishStatus {
    /// 创建
    Created,
    /// 已认领
    Claimed,
    /// 已完成
    Finished,
    /// 已关闭
    Closed,
}
