// 领域层 - 经济系统实体
// FSD.latest.md compliant - 完整的积分/钻石/经验流水模型

use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 用户组内爱心积分账户 - 按user_id+group_id独立计算
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserGroupPoints {
    pub id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub available_love_point: i64,
    pub frozen_love_point: i64,
    pub updated_at: DateTime<Utc>,
}

/// 爱心积分流水 - 所有积分变动必须写流水
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LovePointTransaction {
    pub id: i64,
    pub user_id: i64,
    pub group_id: i64,
    pub type_: String, // EARN, FREEZE, UNFREEZE, DEDUCT, ADJUST
    pub amount: i64,
    pub available_before: i64,
    pub available_after: i64,
    pub frozen_before: i64,
    pub frozen_after: i64,
    pub biz_type: String,
    pub biz_id: Option<i64>,
    pub idempotency_key: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 组经验流水 - 含等级变化
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupExpTransaction {
    pub id: i64,
    pub group_id: i64,
    pub type_: String, // EARN, ADJUST, REVOKE
    pub amount: i64,
    pub exp_before: i64,
    pub exp_after: i64,
    pub level_before: i32,
    pub level_after: i32,
    pub biz_type: String,
    pub biz_id: Option<i64>,
    pub idempotency_key: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 组钻石流水
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiamondTransaction {
    pub id: i64,
    pub group_id: i64,
    pub type_: String, // EARN, CONSUME, ADJUST
    pub amount: i64,
    pub balance_before: i64,
    pub balance_after: i64,
    pub biz_type: String,
    pub biz_id: Option<i64>,
    pub idempotency_key: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 每日奖励上限统计
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyRewardCounter {
    pub id: i64,
    pub stat_date: NaiveDate,
    pub group_id: i64,
    pub user_id: Option<i64>,
    pub love_point_earned: i64,
    pub group_exp_earned: i64,
    pub normal_order_count: i32,
    pub guest_order_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 用户钻石账户 (legacy - 保留兼容)
#[allow(dead_code)]
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

/// 钻石流水记录 (legacy - 保留兼容)
#[allow(dead_code)]
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