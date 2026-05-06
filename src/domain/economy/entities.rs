// 领域层 - 经济系统实体
// 包含用户钻石、流水等数据库记录和 DTO

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 用户钻石账户
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserDiamond {
    pub id: i64,
    pub user_id: i64,
    pub diamond_balance: i32,
    pub total_get: i32,
    pub total_consume: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

/// 钻石流水记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiamondFlow {
    pub id: i64,
    pub flow_no: String,
    pub user_id: i64,
    pub r#type: i16,
    pub scene: String,
    pub diamond_num: i32,
    pub balance_after: i32,
    pub relation_id: Option<i64>,
    pub remark: Option<String>,
    pub create_time: DateTime<Utc>,
}
