// 领域层 - 双人组实体
// FSD.latest.md compliant - 直接buyer_user_id/seller_user_id映射

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 组记录 - FSD v2版本
///
/// 注意:`group_type` / `status` 在数据库里是自定义枚举类型(PG `group_type_enum`、`user_status_enum`),
/// 不能直接 decode 成 Rust 类型。SQL 端已用 `::text` 强转,这里用 String 接收。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupRecord {
    pub group_id: i64,
    pub group_name: Option<String>,
    pub group_type: String,
    pub status: String,
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
    /// 组公共头像（NULL 时前端回退到成员头像）
    pub group_avatar: Option<String>,
    pub level: i32,
    pub exp: i64,
    pub diamond: i64,
    pub footprint_capacity: i32,
    pub footprint_count: i32,
    /// 组升级进度 (按 group_level_configs 算, 每次 get_group 实时算)
    /// 前端用这个画经验条: expInCurrentLevel / (nextLevelRequiredExp - currentLevelRequiredExp)
    pub level_progress: Option<GroupLevelProgress>,
}

/// 组升级进度 (用于画经验条)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupLevelProgress {
    /// 当前等级
    pub current_level: i32,
    /// 当前等级需要的累计 exp (升到这一级的门槛)
    pub current_level_required_exp: i64,
    /// 下一级 (若已满级, 跟 currentLevel 相同)
    pub next_level: i32,
    /// 下一级需要的累计 exp
    pub next_level_required_exp: i64,
    /// 当前等级内的进度 (group.exp - current_level_required_exp)
    pub exp_in_current_level: i64,
    /// 距离下一级还差多少 (next_level_required_exp - group.exp)
    pub exp_to_next_level: i64,
    /// 是否已满级 (没更高等级了)
    pub is_max_level: bool,
}

/// 组等级阶梯配置 (admin 后台维护)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupLevelConfig {
    pub level: i32,
    pub required_exp: i64,
}

/// 更新单个等级配置请求
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupLevelRequest {
    pub required_exp: i64,
}

/// 组等级配置列表响应
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupLevelListResponse {
    pub levels: Vec<GroupLevelConfig>,
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