// 领域层 - 看板实体

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 活动查询 (新: 支持 cursor 翻页, 不再要 start_date/end_date)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct GroupActivityQuery {
    /// cursor: 上一页最后一条的 "createdAt,id" (base64), 第一页不传
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

/// 活动输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupActivityEventOut {
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub actor_user_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    /// 事件关联的实体类型 (order / wish / food / sign)
    pub ref_type: Option<String>,
    /// 事件关联的实体 ID (order_id / wish_id / food_id / sign_id)
    pub ref_id: Option<i64>,
    /// 订单的目标时间 (仅 order 事件有值, 来自 orders.goal_time)
    pub ref_goal_time: Option<DateTime<Utc>>,
    /// 关联实体的"显示名" (心愿的 wish_name, 菜品的 name 等)
    pub ref_name: Option<String>,
    /// 订单包含的菜品名列表 (仅 order 事件有值, 从 order_items + foods 聚合)
    pub ref_food_names: Option<Vec<String>>,
}

/// 活动列表响应 (CursorPage 风格, 跟其他列表接口对齐)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupActivityListResponse {
    pub events: Vec<GroupActivityEventOut>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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