// 领域层 - 双人组实体
// FSD.latest.md compliant - 直接buyer_user_id/seller_user_id映射

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 组记录 - FSD v2版本
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupRecord {
    pub group_id: i64,
    pub group_name: Option<String>,
    pub group_type: String,
    pub status: i16,
    pub invite_code: Option<String>,
    pub diamond: i32,
    pub footprint_capacity: i32,
    pub footprint_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // FSD v2 fields
    #[sqlx(default)]
    pub buyer_user_id: Option<i64>,
    #[sqlx(default)]
    pub seller_user_id: Option<i64>,
    #[sqlx(default)]
    pub level: Option<i32>,
    #[sqlx(default)]
    pub exp: Option<i64>,
    #[sqlx(default)]
    pub settings: Option<serde_json::Value>,
}

/// 组内成员记录
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupMemberRecord {
    pub id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub role_in_group: String,
    pub is_primary: i16,
    pub created_at: DateTime<Utc>,
}

/// 组简要信息
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupSimpleInfo {
    pub group_id: i64,
    pub group_name: Option<String>,
    pub buyer_user_id: Option<i64>,
    pub seller_user_id: Option<i64>,
    pub level: i32,
    pub diamond: i64,
}

/// 组详细信息
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupDetailInfo {
    pub group_id: i64,
    pub group_name: Option<String>,
    pub buyer_user_id: Option<i64>,
    pub seller_user_id: Option<i64>,
    pub buyer_nick_name: Option<String>,
    pub seller_nick_name: Option<String>,
    pub buyer_avatar: Option<String>,
    pub seller_avatar: Option<String>,
    pub level: i32,
    pub exp: i64,
    pub diamond: i64,
    pub footprint_capacity: i32,
    pub footprint_count: i32,
}

/// 履约统计信息
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentStats {
    pub user_id: i64,
    pub fulfillment_total: i32,
    pub fulfillment_finished: i32,
    pub fulfillment_expired: i32,
    pub fulfillment_rate: f64,
    pub avg_fulfillment_hours: f64,
    pub pending_fulfillment_count: i32,
}

/// 组退出结清检查结果
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettlementCheckResult {
    pub can_exit: bool,
    pub pending_orders: i32,
    pub pending_wishes_initiated: i32,
    pub pending_wishes_as_fulfiller: i32,
    pub frozen_love_points: i64,
    pub pending_compensation: i32,
    pub pending_diamond_reward: i32,
    pub reasons: Vec<String>,
}