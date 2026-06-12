// 应用服务层 - 签到事件处理器
// 处理 SignInEvent，发放签到奖励，更新连续签到状态

use crate::domain::event::types::SignInPayload;
use crate::errors::CustomError;
use sqlx::{PgPool, Row};

/// 处理签到事件
/// 当用户签到时触发，发放相应奖励并更新连续签到天数
#[allow(dead_code)]
pub async fn handle_sign_in(db: &PgPool, payload: &SignInPayload) -> Result<(), CustomError> {
    let user_id = payload.user_id;
    let diamonds = payload.diamond_reward;
    let consecutive_days = payload.consecutive_days;

    // 1. 检查是否已经处理过（幂等检查）
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM event_log WHERE event_type = 'SignInEvent' AND ref_type = 'sign' AND ref_id = $1"
    )
    .bind(payload.sign_id)
    .fetch_optional(db)
    .await?;

    if existing.is_some() {
        // 已处理过，直接返回（幂等）
        return Ok(());
    }

    // 2. 获取用户的组信息
    let group_id: Option<i64> = sqlx::query(
        "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = true LIMIT 1"
    )
    .bind(user_id as i64)
    .fetch_optional(db)
    .await?
    .map(|r| r.get("group_id"));

    // 3. 如果有组，更新组的钻石余额
    if let Some(gid) = group_id {
        sqlx::query("UPDATE association_groups SET diamond = diamond + $1 WHERE group_id = $2")
            .bind(diamonds)
            .bind(gid)
            .execute(db)
            .await?;

        // 记录组的钻石流水
        let new_balance: i32 =
            sqlx::query("SELECT diamond FROM association_groups WHERE group_id = $1")
                .bind(gid)
                .fetch_one(db)
                .await?
                .get(0);

        sqlx::query(
            "INSERT INTO diamond_transactions (group_id, type, diamond_num, balance_after, scene) VALUES ($1, $2, $3, $4, 'sign')"
        )
        .bind(gid)
        .bind(1) // type 1 = earn for sign
        .bind(diamonds)
        .bind(new_balance)
        .execute(db)
        .await?;

        // 4. 检查是否双方都已签到，额外奖励
        let today = chrono::Local::now().date_naive();
        let group_member_count: i32 =
            sqlx::query("SELECT COUNT(*) FROM association_group_members WHERE group_id = $1")
                .bind(gid)
                .fetch_one(db)
                .await?
                .get(0);

        if group_member_count == 2 {
            // 获取该组今日签到人数
            let signed_today: i32 = sqlx::query(
                "SELECT COUNT(DISTINCT user_id) FROM sign_in_records WHERE sign_date = $1 AND user_id IN (SELECT user_id FROM association_group_members WHERE group_id = $2)"
            )
            .bind(today)
            .bind(gid)
            .fetch_one(db)
            .await?
            .get(0);

            if signed_today == 2 {
                // 双方都已签到，额外奖励组钻石
                let bonus: i32 = 5; // 额外奖励5钻石
                sqlx::query(
                    "UPDATE association_groups SET diamond = diamond + $1 WHERE group_id = $2",
                )
                .bind(bonus)
                .bind(gid)
                .execute(db)
                .await?;

                eprintln!(
                    "Group {} both members signed today, bonus {} diamonds awarded",
                    gid, bonus
                );
            }
        }
    }

    // 5. 记录签到事件（已完成处理标记）
    println!(
        "SignIn event processed for user_id={}, sign_id={}, consecutive_days={}, diamonds={}",
        user_id, payload.sign_id, consecutive_days, diamonds
    );

    Ok(())
}
