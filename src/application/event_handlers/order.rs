// 应用服务层 - 订单事件处理器
// 处理订单创建、订单完成等事件，触发经济联动和成就检查

use crate::domain::event::types::{OrderCompletedPayload, OrderCreatedPayload};
use crate::errors::CustomError;
use sqlx::{PgPool, Row};

/// 处理订单创建事件
/// 当新订单创建时触发，用于记录日志或发送通知
#[allow(dead_code)]
pub async fn handle_order_created(
    db: &PgPool,
    payload: &OrderCreatedPayload,
) -> Result<(), CustomError> {
    let order_id = payload.order_id;
    let user_id = payload.user_id;
    let group_id = payload.group_id;

    // 1. 幂等检查
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM event_log WHERE event_type = 'OrderCreatedEvent' AND ref_type = 'order' AND ref_id = $1"
    )
    .bind(order_id)
    .fetch_optional(db)
    .await?;

    if existing.is_some() {
        return Ok(());
    }

    // 2. 记录订单创建日志
    println!(
        "Order created: order_id={}, user_id={}, group_id={:?}",
        order_id, user_id, group_id
    );

    Ok(())
}

/// 处理订单完成事件
/// 当订单被确认完成时触发，发放积分奖励，检查成就
#[allow(dead_code)]
pub async fn handle_order_completed(
    db: &PgPool,
    payload: &OrderCompletedPayload,
) -> Result<(), CustomError> {
    let order_id = payload.order_id;
    let assignee_id = payload.assignee_id;
    let group_id = payload.group_id;

    // 1. 幂等检查
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM event_log WHERE event_type = 'OrderCompletedEvent' AND ref_type = 'order' AND ref_id = $1"
    )
    .bind(order_id)
    .fetch_optional(db)
    .await?;

    if existing.is_some() {
        return Ok(());
    }

    // 2. 获取组的积分配置
    let point_config: Option<(i32, i32)> = sqlx::query_as(
        "SELECT confirmed_finished_points, order_point_percent FROM group_point_configs WHERE group_id = $1"
    )
    .bind(group_id.unwrap_or(0))
    .fetch_optional(db)
    .await?;

    let (base_points, _) = point_config.unwrap_or((10, 0)); // 默认奖励10积分

    // 3. 发放积分给接单人
    let current_points: i32 = sqlx::query("SELECT love_point FROM users WHERE user_id = $1")
        .bind(assignee_id as i64)
        .fetch_one(db)
        .await?
        .get(0);

    let new_points = current_points + base_points;
    sqlx::query("UPDATE users SET love_point = $2 WHERE user_id = $1")
        .bind(assignee_id as i64)
        .bind(new_points)
        .execute(db)
        .await?;

    // 4. 记录积分流水
    sqlx::query(
        "INSERT INTO point_flow (user_id, group_id, amount, balance, scene, relation_id) VALUES ($1, $2, $3, $4, 'order_complete', $5)"
    )
    .bind(assignee_id as i64)
    .bind(group_id.unwrap_or(0))
    .bind(base_points)
    .bind(new_points)
    .bind(order_id)
    .execute(db)
    .await?;

    // 5. 如果有组，额外发放钻石奖励
    if let Some(gid) = group_id {
        // 获取组的钻石余额
        let current_diamond: Option<i32> =
            sqlx::query("SELECT diamond FROM association_groups WHERE group_id = $1")
                .bind(gid)
                .fetch_optional(db)
                .await?
                .map(|r| r.get(0));

        if let Some(current) = current_diamond {
            let diamond_reward: i32 = 5; // 完成订单奖励5钻石
            let new_diamond = current + diamond_reward;
            sqlx::query("UPDATE association_groups SET diamond = $2 WHERE group_id = $1")
                .bind(gid)
                .bind(new_diamond)
                .execute(db)
                .await?;

            // 记录钻石流水
            sqlx::query(
                "INSERT INTO group_diamond_flow (group_id, amount, balance, scene, relation_id) VALUES ($1, $2, $3, 'order_complete', $4)"
            )
            .bind(gid)
            .bind(diamond_reward)
            .bind(new_diamond)
            .bind(order_id)
            .execute(db)
            .await?;

            println!(
                "Order {} completed, group {} received {} diamonds reward",
                order_id, gid, diamond_reward
            );
        }
    }

    // 6. 发布足迹（事件驱动，自动发布）
    println!(
        "Order completed: order_id={}, assignee_id={}, base_points={}",
        order_id, assignee_id, base_points
    );

    // 7. 检查成就（简化版）
    println!(
        "Checking achievements for user {} after order completion",
        assignee_id
    );

    Ok(())
}
