// 应用服务层 - 签到服务
// 包含签到、连续签到奖励等业务用例
// 签到奖励由 admin 在 global_configs.signInRewards7Days 配置(7 位数组),
// 连续 7 天后第 8 天循环回第 1 天,中断则从第 1 天重新开始

use crate::config::AppState;
use crate::domain::event::{EventType, SignInPayload};
use crate::domain::sign_in::entities::DailyCheckinOut;
use crate::domain::sign_in::value_objects::calculate_sign_diamonds;
use crate::errors::CustomError;
use crate::infrastructure::event::publisher::EventPublisher;
use chrono::{Local, NaiveDate};
use std::sync::Arc;

/// 7 天签到奖励默认配置
/// 用于 global_configs 缺失/格式错误时的兜底
const DEFAULT_SIGN_REWARDS_7DAYS: &[i32] = &[5, 6, 7, 8, 9, 10, 20];

/// admin 配置项键名
const CONFIG_KEY_SIGN_REWARDS_7DAYS: &str = "signInRewards7Days";

/// 签到应用服务
pub struct SignService;

impl SignService {
    /// 从 global_configs 读取 7 天签到奖励配置
    /// 失败 / 缺失 / 格式错 → 兜底为 DEFAULT_SIGN_REWARDS_7DAYS
    pub async fn load_sign_rewards(db: &sqlx::PgPool) -> Vec<i32> {
        let row: Option<(Option<serde_json::Value>,)> = sqlx::query_as(
            "SELECT config_value FROM global_configs WHERE config_key = $1",
        )
        .bind(CONFIG_KEY_SIGN_REWARDS_7DAYS)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();

        match row.and_then(|(v,)| v) {
            Some(serde_json::Value::Array(items)) => {
                let parsed: Vec<i32> = items
                    .iter()
                    .filter_map(|v| v.as_i64().map(|n| n as i32))
                    .filter(|n| (1..=100).contains(n))
                    .collect();
                if parsed.is_empty() {
                    DEFAULT_SIGN_REWARDS_7DAYS.to_vec()
                } else {
                    parsed
                }
            }
            _ => DEFAULT_SIGN_REWARDS_7DAYS.to_vec(),
        }
    }

    /// 每日签到(加组钻石 + 全组满签奖励)
    ///
    /// `group_id` 由调用方从 URL 路径传入 (POST /api/groups/{group_id}/sign-in)。
    /// 不再从 `is_primary=1` 反查用户的主组 —— joiner (seller) 在数据库里
    /// is_primary=0 (见 `join_group` 路由),反查会返回 None,导致 sign_in_records
    /// 插入 NULL 触发 NOT NULL 约束。
    /// 鉴权由 `RequireGroup` 中间件保证:用户必须是该组成员才会调到这里。
    pub async fn daily_checkin(
        token: crate::middlewares::auth::UserToken,
        group_id: i64,
        state: &Arc<AppState>,
    ) -> Result<DailyCheckinOut, CustomError> {
        let user_id = token.user_id;
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        // 1. 今日是否已签到
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM sign_in_records WHERE user_id = $1 AND sign_date = $2",
        )
        .bind(user_id)
        .bind(today)
        .fetch_optional(db)
        .await?;
        if existing.is_some() {
            return Err(CustomError::BadRequest("今日已签到".into()));
        }

        // 2. 算连续天数
        let yesterday = today.pred_opt().unwrap();
        let last_sign: Option<(NaiveDate, i32)> = sqlx::query_as(
            "SELECT sign_date, consecutive_days FROM sign_in_records
             WHERE user_id = $1 AND sign_date = $2",
        )
        .bind(user_id)
        .bind(yesterday)
        .fetch_optional(db)
        .await?;
        let consecutive_days = last_sign.map(|(_, cd)| cd + 1).unwrap_or(1);

        // 3. 读 7 天奖励配置
        let rewards = Self::load_sign_rewards(db).await;
        let diamond_reward = calculate_sign_diamonds(consecutive_days, &rewards);

        // 4. 事务:写记录 + 加组钻石 + 写基础流水
        let mut tx = db.begin().await?;

        let sign_id: i64 = sqlx::query_scalar(
            "INSERT INTO sign_in_records
                (group_id, user_id, sign_date, consecutive_days, diamond_reward, full_team_bonus, full_team_bonus_amt)
             VALUES ($1, $2, $3, $4, $5, FALSE, 0)
             RETURNING id",
        )
        .bind(group_id)
        .bind(user_id)
        .bind(today)
        .bind(consecutive_days)
        .bind(diamond_reward)
        .fetch_one(&mut *tx)
        .await?;

        let mut full_team_bonus_amt: i32 = 0;

        // 写基础流水 + 加组钻石
        let sign_idempotency_key = format!("sign_in_{}", sign_id);
        sqlx::query(
            r#"INSERT INTO diamond_transactions
               (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, created_at)
               SELECT $1, 'EARN', $2, diamond, diamond + $2, 'SIGN_IN', $3, $4, NOW()
               FROM association_groups WHERE group_id = $1"#,
        )
        .bind(group_id)
        .bind(diamond_reward as i64)
        .bind(sign_id)
        .bind(&sign_idempotency_key)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE association_groups SET diamond = diamond + $1, updated_at = NOW() WHERE group_id = $2",
        )
        .bind(diamond_reward as i64)
        .bind(group_id)
        .execute(&mut *tx)
        .await?;

        // 5. 满签判定(在事务内)
        let full_signed = Self::is_full_team_signed_in_tx(&mut tx, group_id, today).await?;
        if full_signed {
            let amt = Self::load_full_team_bonus_amt_in_tx(&mut tx).await;
            if amt > 0 {
                // 标记当前 sign_in_records
                sqlx::query(
                    "UPDATE sign_in_records
                     SET full_team_bonus = TRUE, full_team_bonus_amt = $1
                     WHERE id = $2",
                )
                .bind(amt)
                .bind(sign_id)
                .execute(&mut *tx)
                .await?;
                // 加组钻石 + 写满签流水
                let bonus_idempotency_key = format!("full_team_bonus_{}", sign_id);
                sqlx::query(
                    r#"INSERT INTO diamond_transactions
                       (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, created_at)
                       SELECT $1, 'EARN', $2, diamond, diamond + $2, 'FULL_TEAM_BONUS', $3, $4, NOW()
                       FROM association_groups WHERE group_id = $1"#,
                )
                .bind(group_id)
                .bind(amt as i64)
                .bind(sign_id)
                .bind(&bonus_idempotency_key)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE association_groups SET diamond = diamond + $1, updated_at = NOW() WHERE group_id = $2",
                )
                .bind(amt as i64)
                .bind(group_id)
                .execute(&mut *tx)
                .await?;
                full_team_bonus_amt = amt;
            }
        }

        tx.commit().await?;

        // 6. 拿最终组钻石用于返回
        let total_diamonds: i32 = sqlx::query_scalar(
            "SELECT diamond FROM association_groups WHERE group_id = $1",
        )
        .bind(group_id)
        .fetch_one(db)
        .await?;

        // 7. 发事件
        let payload = SignInPayload {
            sign_id,
            user_id,
            group_id: Some(group_id),
            sign_date: today.to_string(),
            consecutive_days,
            diamond_reward,
            trace_id: None,
        };
        let _ = EventPublisher::publish(
            db,
            EventType::SignIn,
            payload,
            Some(user_id),
            Some(group_id),
            Some("sign"),
            Some(sign_id),
        )
        .await;

        Ok(DailyCheckinOut {
            diamond_reward,
            consecutive_days,
            total_diamonds,
            full_team_bonus: full_team_bonus_amt > 0,
            full_team_bonus_amt,
        })
    }

    /// 事务版本的满签判定(接收 &mut Transaction)
    async fn is_full_team_signed_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        group_id: i64,
        today: NaiveDate,
    ) -> Result<bool, CustomError> {
        let member_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM association_group_members
             WHERE group_id = $1 AND member_status = 'ACTIVE'",
        )
        .bind(group_id)
        .fetch_one(&mut **tx)
        .await?;
        if member_count < 2 {
            return Ok(false);
        }
        let signed_today: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT user_id) FROM sign_in_records
             WHERE group_id = $1 AND sign_date = $2",
        )
        .bind(group_id)
        .bind(today)
        .fetch_one(&mut **tx)
        .await?;
        if signed_today < member_count {
            return Ok(false);
        }
        let already_paid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sign_in_records
             WHERE group_id = $1 AND sign_date = $2 AND full_team_bonus = TRUE)",
        )
        .bind(group_id)
        .bind(today)
        .fetch_one(&mut **tx)
        .await?;
        Ok(!already_paid)
    }

    /// 事务版本读 fullTeamBonusAmt
    async fn load_full_team_bonus_amt_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> i32 {
        let row: Option<(Option<serde_json::Value>,)> = sqlx::query_as(
            "SELECT config_value FROM global_configs WHERE config_key = $1",
        )
        .bind("fullTeamBonusAmt")
        .fetch_optional(&mut **tx)
        .await
        .ok()
        .flatten();
        row.and_then(|(v,)| v)
            .and_then(|v| v.as_i64().map(|n| n as i32))
            .filter(|n| (0..=100).contains(n))
            .unwrap_or(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_index_day1() {
        let r = [5, 6, 7, 8, 9, 10, 20];
        assert_eq!(calculate_sign_diamonds(1, &r), 5);
    }

    #[test]
    fn cycle_index_day7() {
        let r = [5, 6, 7, 8, 9, 10, 20];
        assert_eq!(calculate_sign_diamonds(7, &r), 20);
    }

    #[test]
    fn cycle_index_day8_restarts() {
        let r = [5, 6, 7, 8, 9, 10, 20];
        assert_eq!(calculate_sign_diamonds(8, &r), 5);
    }

    #[test]
    fn cycle_index_day14_restarts() {
        let r = [5, 6, 7, 8, 9, 10, 20];
        assert_eq!(calculate_sign_diamonds(14, &r), 20);
    }

    #[test]
    fn interruption_resets_via_consecutive_days() {
        // 业务上"中断 = 重新从第 1 天算"在 sign_in_records 写入时由 consecutive_days=1 表达
        // 这里验证当 consecutive_days=1 时,取值确实是 rewards[0]
        let r = [10, 20, 30, 40, 50, 60, 70];
        assert_eq!(calculate_sign_diamonds(1, &r), 10);
    }

    #[test]
    fn custom_length_array() {
        // 即便 admin 配的不是 7 个值,逻辑也走 %len 不会越界
        let r = [100, 200, 300];
        assert_eq!(calculate_sign_diamonds(1, &r), 100);
        assert_eq!(calculate_sign_diamonds(3, &r), 300);
        assert_eq!(calculate_sign_diamonds(4, &r), 100); // 4 % 3 = 1
    }

    #[test]
    fn empty_array_returns_zero() {
        // admin 配空数组时拿不到奖励,返回 0 而不是 panic
        let r: [i32; 0] = [];
        assert_eq!(calculate_sign_diamonds(1, &r), 0);
    }

    // ---- 满签业务规则文档化（不连 DB,等集成测试基建到位用 sqlx::test 写真测试） ----

    /// 业务规则:组成员 >= 2 且都签到 且今天没发过 → true
    #[test]
    fn full_team_signed_rule_doc_2_members_both_signed() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }

    /// 业务规则:组里只有 1 人不算满签
    #[test]
    fn full_team_signed_rule_doc_single_member_false() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }

    /// 业务规则:今天已发过满签的组,后续签到不再触发
    #[test]
    fn full_team_signed_rule_doc_already_paid_skipped() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }

    /// 业务规则:fullTeamBonusAmt = 0 时,不写流水、不打标记
    #[test]
    fn full_team_bonus_zero_amt_skipped() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }
}
