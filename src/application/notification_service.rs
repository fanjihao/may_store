// 应用服务层 - 通知服务
// 包含微信推送等通知业务用例

use crate::config::AppState;
use crate::domain::event::EventType;
use crate::errors::CustomError;
use sqlx::{PgPool, Row};
use std::sync::Arc;

/// 通知类型枚举
#[derive(Debug, Clone, Copy)]
pub enum NotificationType {
    /// 订单创建通知
    OrderCreated,
    /// 订单被接受通知
    OrderAccepted,
    /// 订单完成通知
    OrderCompleted,
    /// 心愿完成通知
    WishFulfilled,
    /// 签到通知
    SignIn,
}

/// 通知服务
/// 负责向用户推送各类通知消息（微信模板消息、系统通知等）
pub struct NotificationService;

impl NotificationService {
    /// 发送订单通知
    /// 根据订单状态变化向相关用户发送微信推送
    pub async fn push_order_notification(
        state: &Arc<AppState>,
        order_id: i64,
        notification_type: NotificationType,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 获取订单相关信息（创建者、接单人）
        let order_info: Option<(i64, Option<i64>, String)> =
            sqlx::query("SELECT user_id, assignee_id, status FROM orders WHERE order_id = $1")
                .bind(order_id)
                .fetch_optional(db)
                .await?
                .map(|r| {
                    (
                        r.get::<i64, _>("user_id"),
                        r.get::<Option<i64>, _>("assignee_id"),
                        r.get::<String, _>("status"),
                    )
                });

        let (creator_id, assignee_id, status) = match order_info {
            Some(info) => info,
            None => return Err(CustomError::NotFound("订单不存在".into())),
        };

        // 根据通知类型决定发送给谁
        match notification_type {
            NotificationType::OrderCreated => {
                // 通知接单人（如果有）有新订单
                if let Some(assignee) = assignee_id {
                    Self::send_template_message(state, assignee, "有新订单待接单").await?;
                }
            }
            NotificationType::OrderAccepted => {
                // 通知下单人已有人接单
                Self::send_template_message(
                    state,
                    creator_id,
                    &format!("订单已被接受: {}", status),
                )
                .await?;
            }
            NotificationType::OrderCompleted => {
                // 通知下单人订单已完成，等待确认
                Self::send_template_message(state, creator_id, "订单已完成，请确认").await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// 发送签到通知
    /// 向用户推送签到结果和奖励信息
    pub async fn push_sign_notification(
        state: &Arc<AppState>,
        user_id: i64,
        consecutive_days: i32,
        diamonds_earned: i32,
    ) -> Result<(), CustomError> {
        let message = if consecutive_days >= 7 {
            format!(
                "太棒了！连续签到{}天，获得{}钻石",
                consecutive_days, diamonds_earned
            )
        } else if consecutive_days >= 3 {
            format!(
                "连续签到{}天，获得{}钻石，继续加油！",
                consecutive_days, diamonds_earned
            )
        } else {
            format!("签到成功，获得{}钻石", diamonds_earned)
        };

        Self::send_template_message(state, user_id, &message).await?;
        Ok(())
    }

    /// 发送心愿完成通知
    /// 向创建者和认领者推送心愿完成消息
    pub async fn push_wish_notification(
        state: &Arc<AppState>,
        wish_id: i64,
        wish_name: &str,
    ) -> Result<(), CustomError> {
        let db = &state.db_pool;

        // 获取心愿信息
        let wish_info: Option<(i64, i64, Option<i64>)> =
            sqlx::query("SELECT created_by, group_id, claimed_by FROM wishes WHERE wish_id = $1")
                .bind(wish_id)
                .fetch_optional(db)
                .await?
                .map(|r| {
                    (
                        r.get::<i64, _>("created_by"),
                        r.get::<i64, _>("group_id"),
                        r.get::<Option<i64>, _>("claimed_by"),
                    )
                });

        if let Some((creator_id, _, claimed_by)) = wish_info {
            // 通知创建者
            Self::send_template_message(
                state,
                creator_id,
                &format!("您的心愿「{}」已完成", wish_name),
            )
            .await?;

            // 通知认领者
            if let Some(claimer) = claimed_by {
                Self::send_template_message(
                    state,
                    claimer,
                    &format!("您已完成心愿「{}」", wish_name),
                )
                .await?;
            }
        }

        Ok(())
    }

    /// 发送系统通知
    /// 通用系统消息推送
    pub async fn push_system_notification(
        state: &Arc<AppState>,
        user_id: i64,
        title: &str,
        content: &str,
    ) -> Result<(), CustomError> {
        Self::send_template_message(state, user_id, &format!("{}: {}", title, content)).await?;
        Ok(())
    }

    // ========== 私有辅助方法 ==========

    /// 发送微信模板消息（简化实现）
    /// 实际项目中应调用微信模板消息API
    async fn send_template_message(
        state: &Arc<AppState>,
        user_id: i64,
        message: &str,
    ) -> Result<(), CustomError> {
        // 获取用户的推送ID
        let push_id: Option<String> = sqlx::query("SELECT push_id FROM users WHERE user_id = $1")
            .bind(user_id as i64)
            .fetch_optional(&state.db_pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| {
                let s: String = r.get("push_id");
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            });

        // 如果有push_id，则发送微信模板消息
        if let Some(pid) = push_id {
            if !pid.is_empty() {
                // TODO: 调用微信模板消息API发送通知
                // 实际实现需要调用微信接口，这里仅记录日志
                println!(
                    "Sending WeChat template message to user_id={}, push_id={}, message={}",
                    user_id, pid, message
                );
            }
        }

        Ok(())
    }

    /// 获取用户的未读通知数量
    pub async fn get_unread_count(db: &PgPool, user_id: i64) -> Result<i32, CustomError> {
        // 简化实现：查询通知表获取未读数
        let count: i32 = sqlx::query(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false",
        )
        .bind(user_id as i64)
        .fetch_one(db)
        .await?
        .get(0);

        Ok(count)
    }

    /// 标记通知为已读
    pub async fn mark_as_read(
        db: &PgPool,
        user_id: i64,
        notification_id: i64,
    ) -> Result<(), CustomError> {
        sqlx::query("UPDATE notifications SET is_read = true WHERE id = $1 AND user_id = $2")
            .bind(notification_id)
            .bind(user_id as i64)
            .execute(db)
            .await?;
        Ok(())
    }
}
