// API - 签到相关 ws 广播
// 签到成功 / 全组满签后, 给组里所有 ACTIVE 成员推 group_diamond_change

use crate::api::ws::get_connection_manager;
use crate::api::ws::messages::{
    WsEnvelope, WsGroupDiamondChangeData,
};
use sqlx::PgPool;

/// 签到成功后, 推"组钻石变了"给全组在线成员
///
/// 设计: 跟 push_group_member_change_notice 一样, 按 group_id 反查所有 ACTIVE
/// 成员 -> 逐个推。target 不在线静默丢弃(业务侧说"暂时不管离线")。
///
/// 调用方: src/application/sign_in_service.rs::daily_checkin
/// 时机: 事务 commit 之后, HTTP 200 返回之前
pub async fn push_group_diamond_change_notice(
    db: &PgPool,
    group_id: i64,
    user_id: i64,
    reason: &str,
    consecutive_days: i32,
) {
    // 拿 group 最新信息
    let group_row: Option<(i32, i32, i32)> = match sqlx::query_as(
        "SELECT diamond, exp, level FROM association_groups WHERE group_id = $1",
    )
    .bind(group_id)
    .fetch_optional(db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::error!(
                "[push_group_diamond_change] 查 group_id={} 失败: {:?}",
                group_id, e
            );
            return;
        }
    };

    let (diamond, exp, level) = match group_row {
        Some(v) => v,
        None => {
            log::error!(
                "[push_group_diamond_change] group_id={} 不存在",
                group_id
            );
            return;
        }
    };

    let payload = WsGroupDiamondChangeData {
        group_id,
        user_id,
        diamond,
        exp,
        level,
        consecutive_days,
        reason: reason.to_string(),
    };

    let envelope = WsEnvelope::group_diamond_change(&payload);
    let Ok(json) = serde_json::to_string(&envelope) else {
        log::error!("组钻石变化通知序列化失败: {:?}", payload);
        return;
    };

    // 按 group_id 反查所有 ACTIVE 成员 -> 全体推送
    let target_ids: Vec<i64> = match sqlx::query_scalar(
        "SELECT user_id FROM association_group_members
         WHERE group_id = $1 AND member_status = 'ACTIVE'",
    )
    .bind(group_id)
    .fetch_all(db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            log::error!(
                "[push_group_diamond_change] 查 group_id={} 成员失败: {:?}",
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
                "组钻石变化通知未送达(用户 {} 不在线): group_id={} reason={}",
                target_id, group_id, reason
            );
        }
    }
}
