// 应用服务层 - 看板服务

use sqlx::PgPool;
use crate::domain::dashboard::*;
use crate::errors::CustomError;

pub struct DashboardService;

impl DashboardService {
    pub async fn get_group_activities(
        db: &PgPool,
        group_id: i64,
        query: &GroupActivityQuery,
    ) -> Result<Vec<GroupActivityEventOut>, CustomError> {
        // TODO: 迁移自 dashboard/service.rs
        todo!("迁移获取组活动")
    }

    pub async fn get_top_food_orders(
        db: &PgPool,
        group_id: i64,
    ) -> Result<TopFoodRankingResponse, CustomError> {
        // TODO: 迁移自 dashboard/service.rs
        todo!("迁移获取热门菜品")
    }

    pub async fn get_my_today_orders(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<TodayOrdersResponse, CustomError> {
        // TODO: 迁移自 dashboard/service.rs
        todo!("迁移获取今日订单")
    }

    pub async fn get_my_order_stats(
        db: &PgPool,
        user_id: i64,
    ) -> Result<OrderStatsOut, CustomError> {
        // TODO: 迁移自 dashboard/service.rs
        todo!("迁移获取订单统计")
    }

    pub async fn get_points_journey(
        db: &PgPool,
        user_id: i64,
    ) -> Result<PointsJourneyOut, CustomError> {
        // TODO: 迁移自 dashboard/service.rs
        todo!("迁移获取积分旅程")
    }

    pub async fn get_week_order_dates(
        db: &PgPool,
        group_id: i64,
    ) -> Result<WeekOrderDatesOut, CustomError> {
        // TODO: 迁移自 dashboard/service.rs
        todo!("迁移获取周订单日期")
    }

    pub async fn get_date_foods(
        db: &PgPool,
        query: &DateQuery,
    ) -> Result<DateFoodsResponse, CustomError> {
        // TODO: 迁移自 dashboard/service.rs
        todo!("迁移获取日期菜品")
    }
}
