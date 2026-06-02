// 领域层 - 成就实体
// 包含成就定义、用户成就等数据库记录

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 成就定义记录
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AchievementDefinition {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub requirement_type: String,
    pub requirement_value: i32,
    pub create_time: DateTime<Utc>,
}

/// 用户成就记录
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserAchievement {
    pub id: i64,
    pub user_id: i64,
    pub achievement_id: i64,
    pub unlocked_at: DateTime<Utc>,
}
