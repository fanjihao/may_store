// 领域层 - 双人组值对象
// FSD.latest.md compliant

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 组角色枚举 - 直接映射到buyer_user_id/seller_user_id
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum GroupRole {
    /// 下单人 (Buyer)
    Buyer,
    /// 接单人 (Seller)
    Seller,
}

/// 组状态枚举
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum GroupStatus {
    /// 活跃
    Active,
    /// 已关闭
    Closed,
}

/// 组类型枚举
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum GroupType {
    /// 固定双人组
    Pair,
    /// 家庭组
    Family,
    /// 团队组
    Team,
}

/// 组配置值对象
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupSettings {
    /// 是否允许带在途心愿互换身份
    pub swap_ignore_ongoing_wish: bool,
    /// 本组普通订单默认爱心积分
    pub normal_order_love_point: Option<i32>,
    /// 本组做客订单默认爱心积分
    pub guest_order_love_point: Option<i32>,
    /// 本组普通订单默认组经验
    pub normal_order_group_exp: Option<i32>,
    /// 本组做客订单默认组经验
    pub guest_order_group_exp: Option<i32>,
    /// 本组用户每日爱心积分上限
    pub daily_love_point_limit: Option<i32>,
    /// 本组每日经验上限
    pub daily_group_exp_limit: Option<i32>,
    /// 本组订单超时时间(小时)
    pub order_timeout_hours: Option<i32>,
    /// 菜品容量
    pub food_capacity: Option<i32>,
    /// 标签容量
    pub tag_capacity: Option<i32>,
    /// 足迹容量
    pub footprint_capacity: Option<i32>,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            swap_ignore_ongoing_wish: false,
            normal_order_love_point: None,
            guest_order_love_point: None,
            normal_order_group_exp: None,
            guest_order_group_exp: None,
            daily_love_point_limit: None,
            daily_group_exp_limit: None,
            order_timeout_hours: None,
            food_capacity: None,
            tag_capacity: None,
            footprint_capacity: None,
        }
    }
}

/// 组等级值对象
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupLevel {
    /// 等级
    pub level: i32,
    /// 升到本级所需经验
    pub exp_threshold: i64,
    /// 每日爱心积分上限加成
    pub daily_love_point_limit_bonus: i32,
    /// 每日经验上限加成
    pub daily_group_exp_limit_bonus: i32,
    /// 容量加成
    pub capacity_bonus: i32,
}