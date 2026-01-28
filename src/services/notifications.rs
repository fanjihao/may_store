use chrono::Utc;
use reqwest::Client;
use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::wx::auth::{fetch_set_access_token, get_access_token};
use crate::{errors::CustomError, models::orders::OrderStatusEnum};

// 订单推送类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderPushType {
    /// 订单创建通知
    Created,
    /// 订单状态更新通知
    StatusUpdated,
}

impl OrderPushType {
    /// 获取对应的模板编码
    fn template_code(&self) -> &'static str {
        match self {
            OrderPushType::Created => "ORDER_CREATED",
            OrderPushType::StatusUpdated => "ORDER_STATUS_UPDATED",
        }
    }
}

// 推送订单状态变更（根据 order_id 查询订单、菜品、用户 push_id 并发送模板消息）
// 失败时只记录日志，不影响主流程。
pub async fn push_order_status(order_id: i64, db_pool: PgPool) -> Result<(), CustomError> {
    push_order_with_type(order_id, OrderPushType::StatusUpdated, db_pool).await
}

/// 根据指定的推送类型发送订单通知
pub async fn push_order_with_type(
    order_id: i64,
    push_type: OrderPushType,
    db_pool: PgPool,
) -> Result<(), CustomError> {
    // 查询订单 + 相关用户 push_id
    let order_row =
        sqlx::query("SELECT order_id, user_id, guest_id, group_id, status, created_at, goal_time FROM orders WHERE order_id=$1")
            .bind(order_id)
            .fetch_optional(&db_pool)
            .await?;
    let row = match order_row {
        Some(r) => r,
        None => return Ok(()),
    };
    let status: OrderStatusEnum = row
        .try_get("status")
        .ok()
        .unwrap_or(OrderStatusEnum::PENDING);
    let created_at: chrono::DateTime<Utc> = row.get("created_at");
    let goal_time: Option<chrono::DateTime<Utc>> = row.try_get("goal_time").ok();

    let user_id: i64 = row.get("user_id");
    let group_id: Option<i64> = row.try_get("group_id").ok();

    // 聚合菜品名称（最多取5个）
    let food_rows = sqlx::query(
        "SELECT f.food_name FROM order_items oi JOIN foods f ON oi.food_id=f.food_id WHERE oi.order_id=$1 LIMIT 5"
    )
        .bind(order_id)
        .fetch_all(&db_pool)
        .await?;
    let mut names: Vec<String> = Vec::new();
    for fr in food_rows {
        names.push(fr.get::<String, _>("food_name"));
    }
    let foods_summary = if names.is_empty() {
        "-".to_string()
    } else {
        names.join(" / ")
    };

    // 确定推送目标用户
    let mut push_ids: Vec<String> = Vec::new();

    match push_type {
        OrderPushType::Created => {
            // 创建订单 -> 发送给团队内的 receiving 用户
            if let Some(gid) = group_id {
                let ids = fetch_group_receiving_openids(gid, &db_pool).await?;
                push_ids.extend(ids);
            }
        }
        OrderPushType::StatusUpdated => {
            match status {
                OrderStatusEnum::ACCEPTED | OrderStatusEnum::REJECTED => {
                    // 接单/拒绝 -> 发送给 ordering 用户 (下单人)
                    if let Some(pid) = fetch_user_openid(user_id, &db_pool).await? {
                        push_ids.push(pid);
                    }
                }
                OrderStatusEnum::CANCELLED
                | OrderStatusEnum::FINISHED
                | OrderStatusEnum::EXPIRED
                | OrderStatusEnum::SystemClosed => {
                    // 取消/完成/过期/关闭 -> 发送给 receiving 用户
                    if let Some(gid) = group_id {
                        let ids = fetch_group_receiving_openids(gid, &db_pool).await?;
                        push_ids.extend(ids);
                    }
                }
                OrderStatusEnum::PENDING => {
                    // 理论上不会走到这里，但如果发生，视为创建 -> receiving
                    if let Some(gid) = group_id {
                        let ids = fetch_group_receiving_openids(gid, &db_pool).await?;
                        push_ids.extend(ids);
                    }
                }
            }
        }
    }

    // 去重，避免重复发送
    push_ids.sort();
    push_ids.dedup();

    if push_ids.is_empty() {
        return Ok(());
    }

    // 从数据库查询模板ID
    let template_code = push_type.template_code();
    let template_row = sqlx::query(
        "SELECT wx_template_id FROM wx_subscription_templates WHERE template_code=$1 AND is_active=1"
    )
    .bind(template_code)
    .fetch_optional(&db_pool)
    .await?;

    let template_id = match template_row {
        Some(r) => r.get::<String, _>("wx_template_id"),
        None => {
            log::warn!("template not found for code: {}", template_code);
            return Ok(());
        }
    };

    // 获取 access_token
    fetch_set_access_token().await?;
    let token_opt = get_access_token().await;
    let Some(access_token) = token_opt else {
        return Ok(());
    };

    let client = Client::new();
    let status_cn = status_to_cn(status);
    let now_str = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    for pid in push_ids {
        // 根据推送类型构建不同的消息数据
        let json_data = match push_type {
            OrderPushType::Created => {
                // 订单创建模板：订单编号、订单信息、订单时间、预定时间
                let order_time = created_at.format("%Y-%m-%d %H:%M").to_string();
                let goal_time_str = goal_time
                    .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "待定".to_string());

                serde_json::json!({
                    "touser": &pid,
                    "template_id": template_id,
                    "page": "/pages/order/order",
                    "data": {
                        "number2": {"value": order_id.to_string()},
                        "thing11": {"value": foods_summary.clone()},
                        "time5": {"value": order_time},
                        "time26": {"value": goal_time_str}
                    }
                })
            }
            OrderPushType::StatusUpdated => {
                // 订单状态更新模板：下单时间、订单内容、订单状态、更新时间
                let order_time = created_at.format("%Y-%m-%d %H:%M").to_string();

                serde_json::json!({
                    "touser": &pid,
                    "template_id": template_id,
                    "page": "/pages/order/order",
                    "data": {
                        "date3": {"value": order_time},
                        "thing1": {"value": foods_summary.clone()},
                        "phrase2": {"value": status_cn},
                        "time20": {"value": now_str.clone()}
                    }
                })
            }
        };

        if let Err(e) = client
            .post(format!(
                "https://api.weixin.qq.com/cgi-bin/message/subscribe/send?access_token={}",
                access_token
            ))
            .json(&json_data)
            .send()
            .await
        {
            log::warn!("push order {:?} send error: {}", push_type, e);
        }
    }

    Ok(())
}

async fn fetch_user_openid(user_id: i64, db_pool: &PgPool) -> Result<Option<String>, CustomError> {
    let row = sqlx::query("SELECT open_id FROM users WHERE user_id=$1")
        .bind(user_id)
        .fetch_optional(db_pool)
        .await?;
    Ok(row
        .and_then(|r| r.try_get::<Option<String>, _>("open_id").ok())
        .flatten())
}

async fn fetch_group_receiving_openids(
    group_id: i64,
    db_pool: &PgPool,
) -> Result<Vec<String>, CustomError> {
    // 查询组内 role='RECEIVING' 的用户 open_id
    // 注意：这里使用 users 表的 role 字段，也可以考虑 association_group_members 的 role_in_group
    // 根据需求描述 "receiving 用户"，假设是指用户的全局角色或组内角色。
    // 这里使用 users 表的 role 字段更符合 "Ordering" / "Receiving" 的 persona 划分。
    let rows = sqlx::query(
        "SELECT u.open_id
         FROM association_group_members agm
         JOIN users u ON agm.user_id = u.user_id
         WHERE agm.group_id = $1 AND u.role = 'RECEIVING' AND u.open_id IS NOT NULL"
    )
    .bind(group_id)
    .fetch_all(db_pool)
    .await?;

    let mut ids = Vec::new();
    for r in rows {
        if let Ok(Some(openid)) = r.try_get::<Option<String>, _>("open_id") {
            if !openid.is_empty() {
                ids.push(openid);
            }
        }
    }
    Ok(ids)
}

fn status_to_cn(s: OrderStatusEnum) -> &'static str {
    match s {
        OrderStatusEnum::PENDING => "待处理",
        OrderStatusEnum::ACCEPTED => "已接单",
        OrderStatusEnum::FINISHED => "已完成",
        OrderStatusEnum::CANCELLED => "已取消",
        OrderStatusEnum::EXPIRED => "已过期",
        OrderStatusEnum::REJECTED => "已拒绝",
        OrderStatusEnum::SystemClosed => "系统关闭",
    }
}
