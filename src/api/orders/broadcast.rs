// API - 订单相关 ws 广播 (2026-07-08 新增)
// 订单状态变更可能引发:
//   1) 接单人 (RECEIVING) 爱心积分变动 → 推 love_point_change 给本人
//   2) 全组经验变动              → 推 group_exp_change 给全组
// 这两个事件都是 update_order_status 在事务 commit 之后调用。

use crate::api::ws::get_connection_manager;
use crate::api::ws::messages::{
    WsEnvelope, WsGroupExpChangeData, WsLovePointChangeData,
};
use sqlx::PgPool;

/// 推"用户爱心积分变化"给本人
///
/// 设计: 跟 push_group_member_change_notice 一样, target 不在线静默丢弃
/// (业务侧说"暂时不管离线", 上线后下次拉 /api/users/me 也能拿到最新值)
///
/// 调用方: src/application/order_service.rs::update_order_status
/// 时机: 事务 commit 之后
pub async fn push_love_point_change_notice(
    db: &PgPool,
    user_id: i64,
    delta: i32,
    reason: &str,
    order_id: Option<i64>,
) {
    // 拿用户最新 love_point
    let current_lp: Option<i32> = sqlx::query_scalar(
        "SELECT love_point FROM users WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    let Some(love_point) = current_lp else {
        log::warn!(
            "[push_love_point_change] user_id={} 不存在或 love_point 查不到, 跳过推送",
            user_id
        );
        return;
    };

    let payload = WsLovePointChangeData {
        user_id,
        love_point,
        delta,
        reason: reason.to_string(),
        order_id,
    };

    let envelope = WsEnvelope::love_point_change(&payload);
    let Ok(json) = serde_json::to_string(&envelope) else {
        log::error!("love_point 变化通知序列化失败: {:?}", payload);
        return;
    };

    let manager = get_connection_manager();
    let sent = manager.send_to_user(user_id, &json).await;
    if !sent {
        log::debug!(
            "[push_love_point_change] 用户 {} 不在线, 推送跳过",
            user_id
        );
    }
}

/// 推"组经验变化"给全组在线成员
///
/// 设计: 同 push_group_diamond_change_notice, 按 group_id 反查所有 ACTIVE 成员 -> 逐个推
///
/// 调用方: src/application/order_service.rs::update_order_status
/// 时机: 事务 commit 之后
pub async fn push_group_exp_change_notice(
    db: &PgPool,
    group_id: i64,
    user_id: i64,
    reason: &str,
) {
    // 拿组最新 exp 和 level
    let row: Option<(i64, i32)> = match sqlx::query_as(
        "SELECT exp, level FROM association_groups WHERE group_id = $1",
    )
    .bind(group_id)
    .fetch_optional(db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::error!(
                "[push_group_exp_change] 查 group_id={} 失败: {:?}",
                group_id, e
            );
            return;
        }
    };

    let (exp, level) = match row {
        Some(v) => v,
        None => {
            log::error!("[push_group_exp_change] group_id={} 不存在", group_id);
            return;
        }
    };

    let payload = WsGroupExpChangeData {
        group_id,
        exp,
        level,
        user_id,
        reason: reason.to_string(),
    };

    let envelope = WsEnvelope::group_exp_change(&payload);
    let Ok(json) = serde_json::to_string(&envelope) else {
        log::error!("group_exp 变化通知序列化失败: {:?}", payload);
        return;
    };

    // 按 group_id 反查所有 ACTIVE 成员 -> 全体推送
    let target_ids: Vec<i64> = match sqlx::query_scalar(
        "SELECT user_id FROM association_group_members
         WHERE group_id = $1 AND member_status = 'ACTIVE'::group_member_status_enum",
    )
    .bind(group_id)
    .fetch_all(db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "[push_group_exp_change] 查 group_id={} 成员失败: {:?}",
                group_id, e
            );
            Vec::new()
        }
    };

    let manager = get_connection_manager();
    for target_id in target_ids {
        let sent = manager.send_to_user(target_id, &json).await;
        if !sent {
            log::debug!(
                "[push_group_exp_change] 用户 {} 不在线, 推送跳过",
                target_id
            );
        }
    }
}