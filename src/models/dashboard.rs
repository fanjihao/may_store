use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// 组活动查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GroupActivityQuery {
    /// 返回条数，默认50，最大200
    pub limit: Option<i64>,
    /// 仅返回该时间点之前的事件（用于下拉分页）
    pub before: Option<DateTime<Utc>>,
}

/// 组活动事件输出
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupActivityEventOut {
    pub event_type: String,
    pub actor_user_id: Option<i64>,
    pub ref_id: Option<i64>,
    pub ref_name: Option<String>,
    pub occurred_at: DateTime<Utc>,
    /// 积分流水相关：本次变动的积分值（正增负减）
    pub point_amount: Option<i32>,
    /// 积分类型（枚举值文本）
    pub point_tx_type: Option<String>,
    /// 变动后余额
    pub point_balance_after: Option<i32>,
}

// ============== Top Ordered Foods Ranking ==============

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopFoodOrderOut {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: String,
    pub order_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopFoodRankingResponse {
    pub list: Vec<TopFoodOrderOut>,
    pub message: Option<String>,
}

// ============== Today's Orders Tree ==============

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayOrderEntryOut {
    pub order_id: i64,
    pub category: String,
    pub foods_text: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayOrdersResponse {
    pub list: Vec<TodayOrderEntryOut>,
    pub message: Option<String>,
}

// ============== Order Stats ==============

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderStatsOut {
    pub total_orders: i64,
    pub finished_orders: i64,
    pub rejected_orders: i64,
}

// ============== Points Journey ==============

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JourneyOrderOut {
    pub order_id: i64,
    pub foods_text: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PointsJourneyOut {
    pub today_orders: Vec<JourneyOrderOut>,
    pub today_points: i64,
    pub current_points: i32,
    pub total_gain_points: i64,
    pub total_cost_points: i64,
    pub message: Option<String>,
}

// ============== Get Week Order Dates ==============

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeekOrderDatesOut {
    pub week_dates: Vec<WeekDateInfo>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeekDateInfo {
    pub date: NaiveDate,
    pub day_of_week: i32, // 1=周一, 7=周日
    pub has_order: bool,
    pub order_count: i32,
}

// ============== Get Foods by Date ==============

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DateFoodOut {
    pub food_id: i64,
    pub food_name: String,
    pub food_photo: Option<String>,
    pub ingredients: Option<String>,
    pub steps: Option<String>,
    pub tag_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DateFoodsResponse {
    pub date: NaiveDate,
    pub foods: Vec<DateFoodOut>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DateQuery {
    /// 日期 (YYYY-MM-DD格式)
    pub date: Option<NaiveDate>,
}
