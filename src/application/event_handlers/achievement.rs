// 应用服务层 - 成就事件处理器
// 检查并解锁用户成就，监听各类事件触发成就判定

use crate::domain::event::EventType;
use crate::errors::CustomError;
use sqlx::{PgPool, Row};

/// 成就检查服务
#[allow(dead_code)]
pub struct AchievementService;

/// 检查并更新用户成就
/// 根据不同事件类型检查对应的成就规则，满足条件则解锁成就
#[allow(dead_code)]
impl AchievementService {
    /// 处理成就检查入口
    pub async fn check_achievements(
        db: &PgPool,
        user_id: i64,
        event_type: EventType,
    ) -> Result<(), CustomError> {
        match event_type {
            // 订单相关事件
            EventType::OrderCreated => {
                // 检查「新订单」成就
                Self::check_new_order_achievement(db, user_id).await?;
            }
            EventType::OrderAccepted => {
                // 检查「接单达人」成就
                Self::check_accept_order_achievement(db, user_id).await?;
            }
            EventType::OrderCompleted => {
                // 检查「完成任务」成就
                Self::check_complete_order_achievement(db, user_id).await?;
            }
            EventType::OrderReviewed => {
                // 检查「评价达人」成就
                Self::check_review_achievement(db, user_id).await?;
            }

            // 签到事件
            EventType::SignIn => {
                // 检查连续签到成就
                Self::check_sign_in_streak_achievement(db, user_id).await?;
            }

            // 心愿事件
            EventType::WishFulfilled => {
                // 检查「心愿达成」成就
                Self::check_wish_fulfilled_achievement(db, user_id).await?;
            }

            // 钻石消费事件
            EventType::DiamondConsumed => {
                // 检查「钻石用户」成就
                Self::check_diamond_spent_achievement(db, user_id).await?;
            }

            // 其他事件暂不处理
            _ => {}
        }

        Ok(())
    }

    /// 检查新订单相关成就
    async fn check_new_order_achievement(db: &PgPool, user_id: i64) -> Result<(), CustomError> {
        // 获取用户创建订单数量
        let order_count: i32 = sqlx::query("SELECT COUNT(*) FROM orders WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        // 根据订单数量解锁不同成就
        let (achievement_code, _, name) = match order_count {
            1 => ("first_order", 1, "首单达成"),
            5 => ("order_amateur", 5, "订单新手"),
            10 => ("order_experienced", 10, "订单老手"),
            50 => ("order_master", 50, "订单大师"),
            _ => return Ok(()), // 不满足任何成就条件
        };

        // 检查是否已解锁
        let existing: i64 = sqlx::query(
            "SELECT COUNT(*) FROM user_achievements ua JOIN achievements a ON ua.achievement_id = a.id WHERE ua.user_id = $1 AND a.code = $2"
        )
        .bind(user_id as i64)
        .bind(achievement_code)
        .fetch_one(db)
        .await?
        .get(0);

        if existing == 0 {
            // 解锁成就
            Self::unlock_achievement(db, user_id, achievement_code, name).await?;
        }

        Ok(())
    }

    /// 检查接单相关成就
    async fn check_accept_order_achievement(db: &PgPool, user_id: i64) -> Result<(), CustomError> {
        let accept_count: i32 = sqlx::query("SELECT COUNT(*) FROM orders WHERE assignee_id = $1")
            .bind(user_id as i64)
            .fetch_one(db)
            .await?
            .get(0);

        let (achievement_code, _, name) = match accept_count {
            1 => ("first_accept", 1, "首次接单"),
            10 => ("accept_regular", 10, "接单常客"),
            50 => ("accept_expert", 50, "接单高手"),
            _ => return Ok(()),
        };

        let existing: i64 = sqlx::query(
            "SELECT COUNT(*) FROM user_achievements ua JOIN achievements a ON ua.achievement_id = a.id WHERE ua.user_id = $1 AND a.code = $2"
        )
        .bind(user_id as i64)
        .bind(achievement_code)
        .fetch_one(db)
        .await?
        .get(0);

        if existing == 0 {
            Self::unlock_achievement(db, user_id, achievement_code, name).await?;
        }

        Ok(())
    }

    /// 检查完成订单相关成就
    async fn check_complete_order_achievement(
        db: &PgPool,
        user_id: i64,
    ) -> Result<(), CustomError> {
        let complete_count: i32 = sqlx::query(
            "SELECT COUNT(*) FROM orders WHERE assignee_id = $1 AND status = 'COMPLETED'",
        )
        .bind(user_id as i64)
        .fetch_one(db)
        .await?
        .get(0);

        let (achievement_code, _, name) = match complete_count {
            1 => ("first_complete", 1, "首次完成"),
            10 => ("complete_regular", 10, "完成达人"),
            _ => return Ok(()),
        };

        let existing: i64 = sqlx::query(
            "SELECT COUNT(*) FROM user_achievements ua JOIN achievements a ON ua.achievement_id = a.id WHERE ua.user_id = $1 AND a.code = $2"
        )
        .bind(user_id as i64)
        .bind(achievement_code)
        .fetch_one(db)
        .await?
        .get(0);

        if existing == 0 {
            Self::unlock_achievement(db, user_id, achievement_code, name).await?;
        }

        Ok(())
    }

    /// 检查签到连续天数成就
    async fn check_sign_in_streak_achievement(
        db: &PgPool,
        user_id: i64,
    ) -> Result<(), CustomError> {
        // 获取用户连续签到天数
        let last_sign: Option<(chrono::NaiveDate, i32)> = sqlx::query_as(
            "SELECT sign_date, consecutive_days FROM sign_in_records WHERE user_id = $1 ORDER BY sign_date DESC LIMIT 1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?;

        let consecutive_days = match last_sign {
            Some((_, days)) => days,
            None => return Ok(()),
        };

        let (achievement_code, _, name) = match consecutive_days {
            3 => ("sign_3_days", 3, "连续签到3天"),
            7 => ("sign_7_days", 7, "连续签到7天"),
            14 => ("sign_14_days", 14, "连续签到14天"),
            30 => ("sign_30_days", 30, "连续签到30天"),
            _ => return Ok(()),
        };

        let existing: i64 = sqlx::query(
            "SELECT COUNT(*) FROM user_achievements ua JOIN achievements a ON ua.achievement_id = a.id WHERE ua.user_id = $1 AND a.code = $2"
        )
        .bind(user_id as i64)
        .bind(achievement_code)
        .fetch_one(db)
        .await?
        .get(0);

        if existing == 0 {
            Self::unlock_achievement(db, user_id, achievement_code, name).await?;
        }

        Ok(())
    }

    /// 检查评价相关成就
    async fn check_review_achievement(_db: &PgPool, _user_id: i64) -> Result<(), CustomError> {
        // 简化实现
        Ok(())
    }

    /// 检查心愿达成成就
    async fn check_wish_fulfilled_achievement(
        _db: &PgPool,
        _user_id: i64,
    ) -> Result<(), CustomError> {
        // 简化实现
        Ok(())
    }

    /// 检查钻石消费成就
    async fn check_diamond_spent_achievement(
        _db: &PgPool,
        _user_id: i64,
    ) -> Result<(), CustomError> {
        // 简化实现
        Ok(())
    }

    /// 解锁成就（内部辅助方法）
    async fn unlock_achievement(
        db: &PgPool,
        user_id: i64,
        achievement_code: &str,
        achievement_name: &str,
    ) -> Result<(), CustomError> {
        // 查找成就ID
        let achievement_id: Option<i64> =
            sqlx::query("SELECT id FROM achievements WHERE code = $1 AND is_active = true")
                .bind(achievement_code)
                .fetch_optional(db)
                .await?
                .map(|r| r.get("id"));

        if let Some(aid) = achievement_id {
            // 插入用户成就记录
            sqlx::query(
                "INSERT INTO user_achievements (user_id, achievement_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
            )
            .bind(user_id as i64)
            .bind(aid)
            .execute(db)
            .await?;

            println!(
                "Achievement unlocked: user_id={}, achievement={}({})",
                user_id, achievement_code, achievement_name
            );
        }

        Ok(())
    }
}
