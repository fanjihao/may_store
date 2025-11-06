use serde::{Deserialize, Serialize};
use utoipa::ToSchema;


#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderCollectOut {
    pub total_orders: Option<i64>,
    pub completed_orders: Option<i64>,
    pub rejected_orders: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TodayPointsOut {
    pub deduct_sum: Option<i64>,
    pub earn_sum: Option<i64>,
    pub today_sum: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderRankingDto {
    pub user_id: i32,
    pub role: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TodayOrderOut {
    pub order_id: i32,
    pub goal_time: Option<chrono::DateTime<chrono::Utc>>,
    pub food_id: Option<i32>,
    pub food_name: Option<String>,
    pub food_photo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LotteryDto {
    /// 用户ID，用于按用户筛选菜品（例如只获取自己的菜品）
    pub user_id: i32,
    /// 需要抽取的菜品类型数组（一个类型返回一个随机菜品）
    pub food_types: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LotteryItemOut {
    pub food_type: i32,
    // 使用新的 FoodOut 结构替换已移除的 FoodApplyStruct
    pub food: Option<crate::models::foods::FoodOut>,
}