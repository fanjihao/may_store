// 领域层 - 看板实体

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 活动查询
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct GroupActivityQuery {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub limit: Option<i64>,
}

/// 活动输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupActivityEventOut {
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 今日订单输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayOrdersResponse {
    pub orders: Vec<serde_json::Value>,
}

/// 订单统计输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderStatsOut {
    pub total_orders: i32,
    pub completed_orders: i32,
    pub pending_orders: i32,
    pub total_points: i32,
}

/// 积分旅程输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PointsJourneyOut {
    pub points_history: Vec<PointEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PointEvent {
    pub event_type: String,
    pub points: i32,
    pub created_at: DateTime<Utc>,
}

/// 周订单日期输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeekOrderDatesOut {
    pub dates: Vec<NaiveDate>,
}

/// 日期菜品查询
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct DateQuery {
    pub date: NaiveDate,
    pub group_id: Option<i64>,
}

/// 日期菜品响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DateFoodsResponse {
    pub foods: Vec<serde_json::Value>,
}

/// 热门菜品排名
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopFoodRankingResponse {
    pub rankings: Vec<FoodRanking>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodRanking {
    pub food_id: i64,
    pub food_name: String,
    pub order_count: i32,
}