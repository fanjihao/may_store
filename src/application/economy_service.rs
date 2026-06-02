// 应用服务 - 经济系统服务
// FSD.latest.md compliant - 爱心积分/组经验/组钻石流水管理
// 所有经济变动必须写流水，禁止直接改余额

use chrono::Utc;
use sqlx::PgPool;
use crate::domain::economy::*;
use crate::errors::CustomError;

/// 经济服务 - 处理所有积分、钻石、经验的变动
/// 核心原则:
/// - 所有经济变动必须写流水
/// - 使用幂等键防止重复发放
/// - 检查每日上限
/// - 记录trace_id用于链路追踪
#[allow(dead_code)]
pub struct EconomyService;

#[allow(dead_code)]
impl EconomyService {
    /// 冻结爱心积分 - 心愿选择时调用
    /// 生成FREEZE流水，减少available，增加frozen
    pub async fn freeze_love_points(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        amount: i64,
        biz_type: &str,
        biz_id: i64,
        idempotency_key: &str,
        trace_id: Option<&str>,
    ) -> Result<UserGroupPoints, CustomError> {
        // 检查幂等键是否已使用
        let existing: Option<LovePointTransaction> = sqlx::query_as(
            "SELECT * FROM love_point_transactions WHERE idempotency_key = $1"
        )
        .bind(idempotency_key)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            // 幂等返回 - 已处理过
            return Self::get_user_group_points(db, user_id, group_id).await;
        }

        // 获取当前余额
        let points = Self::get_or_create_user_group_points(db, user_id, group_id).await?;

        if points.available_love_point < amount as i64 {
            return Err(CustomError::BadRequest("爱心积分不足".to_string()));
        }

        let new_available = points.available_love_point - amount as i64;
        let new_frozen = points.frozen_love_point + amount as i64;

        // 更新余额
        sqlx::query(
            "UPDATE user_group_points SET available_love_point = $1, frozen_love_point = $2, updated_at = NOW() WHERE user_id = $3 AND group_id = $4"
        )
        .bind(new_available)
        .bind(new_frozen)
        .bind(user_id)
        .bind(group_id)
        .execute(db)
        .await?;

        // 写流水
        let tx = LovePointTransaction {
            id: 0,
            user_id,
            group_id,
            type_: "FREEZE".to_string(),
            amount: amount as i64,
            available_before: points.available_love_point,
            available_after: new_available,
            frozen_before: points.frozen_love_point,
            frozen_after: new_frozen,
            biz_type: biz_type.to_string(),
            biz_id: Some(biz_id),
            idempotency_key: Some(idempotency_key.to_string()),
            trace_id: trace_id.map(|s| s.to_string()),
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, idempotency_key, trace_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())
            "#
        )
        .bind(tx.user_id)
        .bind(tx.group_id)
        .bind(&tx.type_)
        .bind(tx.amount)
        .bind(tx.available_before)
        .bind(tx.available_after)
        .bind(tx.frozen_before)
        .bind(tx.frozen_after)
        .bind(&tx.biz_type)
        .bind(tx.biz_id)
        .bind(&tx.idempotency_key)
        .bind(&tx.trace_id)
        .execute(db)
        .await?;

        Self::get_user_group_points(db, user_id, group_id).await
    }

    /// 解冻爱心积分 - 心愿逾期或关闭时调用
    /// 生成UNFREEZE流水，减少frozen，增加available
    pub async fn unfreeze_love_points(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        amount: i64,
        biz_type: &str,
        biz_id: i64,
        idempotency_key: &str,
        trace_id: Option<&str>,
    ) -> Result<UserGroupPoints, CustomError> {
        // 检查幂等键
        let existing: Option<LovePointTransaction> = sqlx::query_as(
            "SELECT * FROM love_point_transactions WHERE idempotency_key = $1"
        )
        .bind(idempotency_key)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            return Self::get_user_group_points(db, user_id, group_id).await;
        }

        let points = Self::get_user_group_points(db, user_id, group_id).await?;

        if points.frozen_love_point < amount as i64 {
            return Err(CustomError::BadRequest("冻结积分不足".to_string()));
        }

        let new_available = points.available_love_point + amount as i64;
        let new_frozen = points.frozen_love_point - amount as i64;

        sqlx::query(
            "UPDATE user_group_points SET available_love_point = $1, frozen_love_point = $2, updated_at = NOW() WHERE user_id = $3 AND group_id = $4"
        )
        .bind(new_available)
        .bind(new_frozen)
        .bind(user_id)
        .bind(group_id)
        .execute(db)
        .await?;

        let tx = LovePointTransaction {
            id: 0,
            user_id,
            group_id,
            type_: "UNFREEZE".to_string(),
            amount: amount as i64,
            available_before: points.available_love_point,
            available_after: new_available,
            frozen_before: points.frozen_love_point,
            frozen_after: new_frozen,
            biz_type: biz_type.to_string(),
            biz_id: Some(biz_id),
            idempotency_key: Some(idempotency_key.to_string()),
            trace_id: trace_id.map(|s| s.to_string()),
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, idempotency_key, trace_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())
            "#
        )
        .bind(tx.user_id)
        .bind(tx.group_id)
        .bind(&tx.type_)
        .bind(tx.amount)
        .bind(tx.available_before)
        .bind(tx.available_after)
        .bind(tx.frozen_before)
        .bind(tx.frozen_after)
        .bind(&tx.biz_type)
        .bind(tx.biz_id)
        .bind(&tx.idempotency_key)
        .bind(&tx.trace_id)
        .execute(db)
        .await?;

        Self::get_user_group_points(db, user_id, group_id).await
    }

    /// 扣减冻结积分 - 心愿打卡完成时调用
    /// 生成DEDUCT流水，减少frozen
    pub async fn deduct_frozen_points(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        amount: i64,
        biz_type: &str,
        biz_id: i64,
        idempotency_key: &str,
        trace_id: Option<&str>,
    ) -> Result<UserGroupPoints, CustomError> {
        let existing: Option<LovePointTransaction> = sqlx::query_as(
            "SELECT * FROM love_point_transactions WHERE idempotency_key = $1"
        )
        .bind(idempotency_key)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            return Self::get_user_group_points(db, user_id, group_id).await;
        }

        let points = Self::get_user_group_points(db, user_id, group_id).await?;

        if points.frozen_love_point < amount as i64 {
            return Err(CustomError::BadRequest("冻结积分不足".to_string()));
        }

        let new_frozen = points.frozen_love_point - amount as i64;

        sqlx::query(
            "UPDATE user_group_points SET frozen_love_point = $1, updated_at = NOW() WHERE user_id = $2 AND group_id = $3"
        )
        .bind(new_frozen)
        .bind(user_id)
        .bind(group_id)
        .execute(db)
        .await?;

        let tx = LovePointTransaction {
            id: 0,
            user_id,
            group_id,
            type_: "DEDUCT".to_string(),
            amount: amount as i64,
            available_before: points.available_love_point,
            available_after: points.available_love_point,
            frozen_before: points.frozen_love_point,
            frozen_after: new_frozen,
            biz_type: biz_type.to_string(),
            biz_id: Some(biz_id),
            idempotency_key: Some(idempotency_key.to_string()),
            trace_id: trace_id.map(|s| s.to_string()),
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, idempotency_key, trace_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())
            "#
        )
        .bind(tx.user_id)
        .bind(tx.group_id)
        .bind(&tx.type_)
        .bind(tx.amount)
        .bind(tx.available_before)
        .bind(tx.available_after)
        .bind(tx.frozen_before)
        .bind(tx.frozen_after)
        .bind(&tx.biz_type)
        .bind(tx.biz_id)
        .bind(&tx.idempotency_key)
        .bind(&tx.trace_id)
        .execute(db)
        .await?;

        Self::get_user_group_points(db, user_id, group_id).await
    }

    /// 发放爱心积分 - 订单完成时调用(Seller获得)
    /// 生成EARN流水，增加available
    pub async fn award_love_points(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        amount: i64,
        biz_type: &str,
        biz_id: i64,
        idempotency_key: &str,
        trace_id: Option<&str>,
    ) -> Result<UserGroupPoints, CustomError> {
        let existing: Option<LovePointTransaction> = sqlx::query_as(
            "SELECT * FROM love_point_transactions WHERE idempotency_key = $1"
        )
        .bind(idempotency_key)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            return Self::get_user_group_points(db, user_id, group_id).await;
        }

        let points = Self::get_or_create_user_group_points(db, user_id, group_id).await?;

        let new_available = points.available_love_point + amount as i64;

        sqlx::query(
            "UPDATE user_group_points SET available_love_point = $1, updated_at = NOW() WHERE user_id = $2 AND group_id = $3"
        )
        .bind(new_available)
        .bind(user_id)
        .bind(group_id)
        .execute(db)
        .await?;

        let tx = LovePointTransaction {
            id: 0,
            user_id,
            group_id,
            type_: "EARN".to_string(),
            amount: amount as i64,
            available_before: points.available_love_point,
            available_after: new_available,
            frozen_before: points.frozen_love_point,
            frozen_after: points.frozen_love_point,
            biz_type: biz_type.to_string(),
            biz_id: Some(biz_id),
            idempotency_key: Some(idempotency_key.to_string()),
            trace_id: trace_id.map(|s| s.to_string()),
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, idempotency_key, trace_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())
            "#
        )
        .bind(tx.user_id)
        .bind(tx.group_id)
        .bind(&tx.type_)
        .bind(tx.amount)
        .bind(tx.available_before)
        .bind(tx.available_after)
        .bind(tx.frozen_before)
        .bind(tx.frozen_after)
        .bind(&tx.biz_type)
        .bind(tx.biz_id)
        .bind(&tx.idempotency_key)
        .bind(&tx.trace_id)
        .execute(db)
        .await?;

        Self::get_user_group_points(db, user_id, group_id).await
    }

    /// 发放组经验 - 订单完成时调用
    /// 返回 (new_exp, new_level)
    pub async fn award_group_exp(
        db: &PgPool,
        group_id: i64,
        amount: i64,
        biz_type: &str,
        biz_id: i64,
        idempotency_key: &str,
        trace_id: Option<&str>,
    ) -> Result<(i64, i32), CustomError> {
        let existing: Option<GroupExpTransaction> = sqlx::query_as(
            "SELECT * FROM group_exp_transactions WHERE idempotency_key = $1"
        )
        .bind(idempotency_key)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            // 获取当前等级
            let group: (i64, i32) = sqlx::query_as(
                "SELECT exp, level FROM association_groups WHERE group_id = $1"
            )
            .bind(group_id)
            .fetch_one(db)
            .await?;
            return Ok(group);
        }

        // 获取组当前信息
        let group: (i64, i32, i64) = sqlx::query_as(
            "SELECT exp, level, diamond FROM association_groups WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_one(db)
        .await?;

        let (current_exp, current_level, current_diamond) = group;
        let exp_before = current_exp;
        let level_before = current_level;

        let new_exp = current_exp + amount as i64;

        // TODO: 计算新等级 (需要查group_level_exp_table配置)
        // 简化: 每1000经验升一级
        let new_level = ((new_exp / 1000) + 1).max(current_level as i64) as i32;

        sqlx::query(
            "UPDATE association_groups SET exp = $1, level = $2, updated_at = NOW() WHERE group_id = $3"
        )
        .bind(new_exp)
        .bind(new_level)
        .bind(group_id)
        .execute(db)
        .await?;

        let tx = GroupExpTransaction {
            id: 0,
            group_id,
            type_: "EARN".to_string(),
            amount: amount as i64,
            exp_before,
            exp_after: new_exp,
            level_before,
            level_after: new_level,
            biz_type: biz_type.to_string(),
            biz_id: Some(biz_id),
            idempotency_key: Some(idempotency_key.to_string()),
            trace_id: trace_id.map(|s| s.to_string()),
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO group_exp_transactions (group_id, type, amount, exp_before, exp_after, level_before, level_after, biz_type, biz_id, idempotency_key, trace_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW())
            "#
        )
        .bind(tx.group_id)
        .bind(&tx.type_)
        .bind(tx.amount)
        .bind(tx.exp_before)
        .bind(tx.exp_after)
        .bind(tx.level_before)
        .bind(tx.level_after)
        .bind(&tx.biz_type)
        .bind(tx.biz_id)
        .bind(&tx.idempotency_key)
        .bind(&tx.trace_id)
        .execute(db)
        .await?;

        Ok((new_exp, new_level))
    }

    /// 发放组钻石
    pub async fn award_diamond(
        db: &PgPool,
        group_id: i64,
        amount: i64,
        biz_type: &str,
        biz_id: i64,
        idempotency_key: &str,
        trace_id: Option<&str>,
    ) -> Result<i64, CustomError> {
        let existing: Option<DiamondTransaction> = sqlx::query_as(
            "SELECT * FROM diamond_transactions WHERE idempotency_key = $1"
        )
        .bind(idempotency_key)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            let group: (i64,) = sqlx::query_as(
                "SELECT diamond FROM association_groups WHERE group_id = $1"
            )
            .bind(group_id)
            .fetch_one(db)
            .await?;
            return Ok(group.0);
        }

        let group: (i64,) = sqlx::query_as(
            "SELECT diamond FROM association_groups WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_one(db)
        .await?;

        let balance_before = group.0;
        let balance_after = balance_before + amount as i64;

        sqlx::query(
            "UPDATE association_groups SET diamond = $1, updated_at = NOW() WHERE group_id = $2"
        )
        .bind(balance_after)
        .bind(group_id)
        .execute(db)
        .await?;

        let tx = DiamondTransaction {
            id: 0,
            group_id,
            type_: "EARN".to_string(),
            amount: amount as i64,
            balance_before,
            balance_after,
            biz_type: biz_type.to_string(),
            biz_id: Some(biz_id),
            idempotency_key: Some(idempotency_key.to_string()),
            trace_id: trace_id.map(|s| s.to_string()),
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO diamond_transactions (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, trace_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
            "#
        )
        .bind(tx.group_id)
        .bind(&tx.type_)
        .bind(tx.amount)
        .bind(tx.balance_before)
        .bind(tx.balance_after)
        .bind(&tx.biz_type)
        .bind(tx.biz_id)
        .bind(&tx.idempotency_key)
        .bind(&tx.trace_id)
        .execute(db)
        .await?;

        Ok(balance_after)
    }

    /// 检查每日奖励上限
    /// 返回是否可以发放奖励，以及当前已发放数量
    pub async fn check_daily_reward_limit(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        order_type: &str, // "NORMAL" or "GUEST"
    ) -> Result<DailyRewardCounter, CustomError> {
        let today = Utc::now().date_naive();

        let counter: Option<DailyRewardCounter> = sqlx::query_as(
            "SELECT * FROM daily_reward_counters WHERE stat_date = $1 AND group_id = $2 AND user_id = $3"
        )
        .bind(today)
        .bind(group_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        match counter {
            Some(c) => Ok(c),
            None => {
                // 创建新的每日计数器
                let new_counter = DailyRewardCounter {
                    id: 0,
                    stat_date: today,
                    group_id,
                    user_id: Some(user_id),
                    love_point_earned: 0,
                    group_exp_earned: 0,
                    normal_order_count: 0,
                    guest_order_count: 0,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };

                sqlx::query(
                    r#"
                    INSERT INTO daily_reward_counters (stat_date, group_id, user_id, love_point_earned, group_exp_earned, normal_order_count, guest_order_count, created_at, updated_at)
                    VALUES ($1, $2, $3, 0, 0, 0, 0, NOW(), NOW())
                    ON CONFLICT (stat_date, group_id, user_id) DO NOTHING
                    "#
                )
                .bind(today)
                .bind(group_id)
                .bind(user_id)
                .execute(db)
                .await?;

                // 重新获取
                let c = sqlx::query_as(
                    "SELECT * FROM daily_reward_counters WHERE stat_date = $1 AND group_id = $2 AND user_id = $3"
                )
                .bind(today)
                .bind(group_id)
                .bind(user_id)
                .fetch_one(db)
                .await?;

                Ok(c)
            }
        }
    }

    /// 获取用户组内积分
    pub async fn get_user_group_points(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<UserGroupPoints, CustomError> {
        let points = sqlx::query_as(
            "SELECT * FROM user_group_points WHERE user_id = $1 AND group_id = $2"
        )
        .bind(user_id)
        .bind(group_id)
        .fetch_optional(db)
        .await?;

        match points {
            Some(p) => Ok(p),
            None => Ok(UserGroupPoints {
                id: 0,
                user_id,
                group_id,
                available_love_point: 0,
                frozen_love_point: 0,
                updated_at: Utc::now(),
            }),
        }
    }

    /// 获取或创建用户组内积分账户
    async fn get_or_create_user_group_points(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
    ) -> Result<UserGroupPoints, CustomError> {
        let points = Self::get_user_group_points(db, user_id, group_id).await?;

        if points.id == 0 {
            sqlx::query(
                "INSERT INTO user_group_points (user_id, group_id, available_love_point, frozen_love_point, updated_at) VALUES ($1, $2, 0, 0, NOW())"
            )
            .bind(user_id)
            .bind(group_id)
            .execute(db)
            .await?;

            return Self::get_user_group_points(db, user_id, group_id).await;
        }

        Ok(points)
    }

    /// 更新每日奖励计数器
    pub async fn update_daily_reward_counter(
        db: &PgPool,
        user_id: i64,
        group_id: i64,
        love_point_amount: i64,
        group_exp_amount: i64,
        order_type: &str,
    ) -> Result<(), CustomError> {
        let today = Utc::now().date_naive();

        if order_type == "NORMAL" {
            sqlx::query(
                "UPDATE daily_reward_counters SET love_point_earned = love_point_earned + $1, group_exp_earned = group_exp_earned + $2, normal_order_count = normal_order_count + 1, updated_at = NOW() WHERE stat_date = $3 AND group_id = $4 AND user_id = $5"
            )
            .bind(love_point_amount)
            .bind(group_exp_amount)
            .bind(today)
            .bind(group_id)
            .bind(user_id)
            .execute(db)
            .await?;
        } else {
            sqlx::query(
                "UPDATE daily_reward_counters SET love_point_earned = love_point_earned + $1, group_exp_earned = group_exp_earned + $2, guest_order_count = guest_order_count + 1, updated_at = NOW() WHERE stat_date = $3 AND group_id = $4 AND user_id = $5"
            )
            .bind(love_point_amount)
            .bind(group_exp_amount)
            .bind(today)
            .bind(group_id)
            .bind(user_id)
            .execute(db)
            .await?;
        }

        Ok(())
    }
}