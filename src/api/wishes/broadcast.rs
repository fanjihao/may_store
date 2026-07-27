use sqlx::PgPool;

use crate::api::ws::get_connection_manager;
use crate::api::ws::messages::{WsEnvelope, WsWishUpdateData};

pub async fn push_wish_quality_review_notice(
    db: &PgPool,
    wish_id: i64,
    group_id: i64,
    quality_level: &str,
    diamond_reward: i32,
) {
    let payload = WsWishUpdateData {
        wish_id,
        group_id,
        action: "quality_reviewed".to_string(),
        quality_level: Some(quality_level.to_string()),
        diamond_reward: Some(diamond_reward),
        message: if diamond_reward > 0 {
            format!("心愿获得质量评价，双人组增加 {} 钻石", diamond_reward)
        } else {
            "心愿质量评价已完成".to_string()
        },
    };
    let envelope = WsEnvelope::wish_update(&payload);
    let Ok(json) = serde_json::to_string(&envelope) else {
        log::error!("心愿质量评价通知序列化失败: {:?}", payload);
        return;
    };
    let user_ids: Vec<i64> = match sqlx::query_scalar(
        "SELECT user_id FROM association_group_members \
         WHERE group_id = $1 AND member_status = 'ACTIVE'::group_member_status_enum",
    )
    .bind(group_id)
    .fetch_all(db)
    .await
    {
        Ok(user_ids) => user_ids,
        Err(error) => {
            log::error!(
                "查询心愿通知目标失败: group_id={}, error={}",
                group_id,
                error
            );
            return;
        }
    };
    let manager = get_connection_manager();
    for user_id in user_ids {
        manager.send_to_user(user_id, &json).await;
    }
}
