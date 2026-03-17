use chrono::Utc;
use reqwest::Client;
use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::wx::service::WxService;
use crate::{errors::CustomError, orders::models::OrderStatusEnum};

// 订单推送类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderPushType {
    /// 订单创建通知
    Created,
    /// 订单状态更新通知
    StatusUpdated,
}

impl OrderPushType {
    /// 获取对应的模板编码（小程序）
    fn mp_template_code(&self) -> &'static str {
        match self {
            OrderPushType::Created => "ORDER_CREATED",
            OrderPushType::StatusUpdated => "ORDER_STATUS_UPDATED",
        }
    }

    /// 获取对应的模板编码（公众号）
    fn official_template_code(&self) -> &'static str {
        match self {
            OrderPushType::Created => "OFFICAL_CREATED",
            OrderPushType::StatusUpdated => "OFFICAL_UPDATED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PushTarget {
    openid: String,
    is_official: bool,
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
        .unwrap_or(OrderStatusEnum::PendingAccept);
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
    let mut targets: Vec<PushTarget> = Vec::new();

    match push_type {
        OrderPushType::Created => {
            // 创建订单 -> 发送给团队内的 receiving 用户
            if let Some(gid) = group_id {
                let tgs = fetch_group_receiving_openids(gid, &db_pool).await?;
                targets.extend(tgs);
            }
        }
        OrderPushType::StatusUpdated => {
            match status {
                OrderStatusEnum::InProgress
                | OrderStatusEnum::Rejected
                | OrderStatusEnum::BreederFinished
                | OrderStatusEnum::BreederClosed
                | OrderStatusEnum::Timeout => {
                    // B 触发或系统触发 -> 通知 A
                    if let Some(tg) = fetch_user_openid(user_id, &db_pool).await? {
                        targets.push(tg);
                    }
                }
                OrderStatusEnum::Cancelled
                | OrderStatusEnum::ConfirmedFinished
                | OrderStatusEnum::ConfirmedUnfinished
                | OrderStatusEnum::SystemClosed => {
                    // A 触发或系统触发 -> 通知 B
                    if let Some(gid) = group_id {
                        let tgs = fetch_group_receiving_openids(gid, &db_pool).await?;
                        targets.extend(tgs);
                    }
                }
                OrderStatusEnum::PendingAccept => {
                    if let Some(gid) = group_id {
                        let tgs = fetch_group_receiving_openids(gid, &db_pool).await?;
                        targets.extend(tgs);
                    }
                }
            }
        }
    }

    // 去重
    targets.sort();
    targets.dedup();

    if targets.is_empty() {
        return Ok(());
    }

    let client = Client::new();
    let status_cn = status_to_cn(status);
    let now_str = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 分离小程序目标和公众号目标
    let mp_targets: Vec<&PushTarget> = targets.iter().filter(|t| !t.is_official).collect();
    let official_targets: Vec<&PushTarget> = targets.iter().filter(|t| t.is_official).collect();

    // --- 处理小程序推送 ---
    if !mp_targets.is_empty() {
        let template_code = push_type.mp_template_code();
        let template_row = sqlx::query(
            "SELECT wx_template_id FROM wx_subscription_templates WHERE template_code=$1 AND is_active=1"
        )
        .bind(template_code)
        .fetch_optional(&db_pool)
        .await?;

        if let Some(r) = template_row {
            let template_id: String = r.get("wx_template_id");
            WxService::fetch_set_mp_token().await?;
            if let Some(access_token) = WxService::get_mp_token().await {
                for tg in mp_targets {
                    let json_data = match push_type {
                        OrderPushType::Created => {
                            let order_time = created_at.format("%Y-%m-%d %H:%M").to_string();
                            let goal_time_str = goal_time
                                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_else(|| "待定".to_string());
                            serde_json::json!({
                                "touser": &tg.openid,
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
                            let order_time = created_at.format("%Y-%m-%d %H:%M").to_string();
                            serde_json::json!({
                                "touser": &tg.openid,
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
                    let res = client
                        .post(format!(
                            "https://api.weixin.qq.com/cgi-bin/message/subscribe/send?access_token={}",
                            access_token
                        ))
                        .json(&json_data)
                        .send()
                        .await;
                    if let Err(e) = res {
                        println!("push mp error: {}", e);
                    }
                }
            }
        }
    }

    // --- 处理公众号推送 ---
    if !official_targets.is_empty() {
        let template_code = push_type.official_template_code();
        let template_row = sqlx::query(
            "SELECT wx_template_id FROM wx_subscription_templates WHERE template_code=$1 AND is_active=1"
        )
        .bind(template_code)
        .fetch_optional(&db_pool)
        .await?;

        if let Some(r) = template_row {
            let template_id: String = r.get("wx_template_id");
            WxService::fetch_set_access_token().await?;
            if let Some(access_token) = WxService::get_access_token().await {
                for tg in official_targets {
                    let json_data = match push_type {
                        OrderPushType::Created => {
                            let order_time = created_at.format("%Y-%m-%d %H:%M").to_string();
                            let goal_time_str = goal_time
                                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_else(|| "待定".to_string());
                            serde_json::json!({
                                "touser": &tg.openid,
                                "template_id": template_id,
                                "data": {
                                    "number2": {"value": order_id.to_string()},
                                    "thing11": {"value": foods_summary.clone()},
                                    "time5": {"value": order_time},
                                    "time26": {"value": goal_time_str}
                                }
                            })
                        }
                        OrderPushType::StatusUpdated => {
                            let order_time = created_at.format("%Y-%m-%d %H:%M").to_string();
                            serde_json::json!({
                                "touser": &tg.openid,
                                "template_id": template_id,
                                "data": {
                                    "date3": {"value": order_time},
                                    "thing1": {"value": foods_summary.clone()},
                                    "phrase2": {"value": status_cn},
                                    "time20": {"value": now_str.clone()}
                                }
                            })
                        }
                    };
                    let res = client
                        .post(format!(
                            "https://api.weixin.qq.com/cgi-bin/message/template/send?access_token={}",
                            access_token
                        ))
                        .json(&json_data)
                        .send()
                        .await;
                    if let Err(e) = res {
                        println!("push official error: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn fetch_user_openid(
    user_id: i64,
    db_pool: &PgPool,
) -> Result<Option<PushTarget>, CustomError> {
    let row = sqlx::query("SELECT open_id, push_id FROM users WHERE user_id=$1")
        .bind(user_id)
        .fetch_optional(db_pool)
        .await?;

    if let Some(r) = row {
        // 优先使用 push_id
        if let Ok(Some(pid)) = r.try_get::<Option<String>, _>("push_id") {
            if !pid.is_empty() {
                return Ok(Some(PushTarget {
                    openid: pid,
                    is_official: true,
                }));
            }
        }
        // 否则使用 open_id
        if let Ok(Some(oid)) = r.try_get::<Option<String>, _>("open_id") {
            if !oid.is_empty() {
                return Ok(Some(PushTarget {
                    openid: oid,
                    is_official: false,
                }));
            }
        }
    }
    Ok(None)
}

async fn fetch_group_receiving_openids(
    group_id: i64,
    db_pool: &PgPool,
) -> Result<Vec<PushTarget>, CustomError> {
    // 查询组内 role='RECEIVING' 的用户
    let rows = sqlx::query(
        "SELECT u.open_id, u.push_id
         FROM association_group_members agm
         JOIN users u ON agm.user_id = u.user_id
         WHERE agm.group_id = $1 AND u.role = 'RECEIVING'",
    )
    .bind(group_id)
    .fetch_all(db_pool)
    .await?;

    let mut targets = Vec::new();
    for r in rows {
        // 优先 push_id
        let push_id: Option<String> = r.try_get("push_id").ok().flatten();
        if let Some(pid) = push_id {
            if !pid.is_empty() {
                targets.push(PushTarget {
                    openid: pid,
                    is_official: true,
                });
                continue;
            }
        }

        let open_id: Option<String> = r.try_get("open_id").ok().flatten();
        if let Some(oid) = open_id {
            if !oid.is_empty() {
                targets.push(PushTarget {
                    openid: oid,
                    is_official: false,
                });
            }
        }
    }
    Ok(targets)
}

fn status_to_cn(s: OrderStatusEnum) -> &'static str {
    match s {
        OrderStatusEnum::PendingAccept => "待接单",
        OrderStatusEnum::InProgress => "进行中",
        OrderStatusEnum::Rejected => "已拒绝",
        OrderStatusEnum::BreederFinished => "已完成(待确认)",
        OrderStatusEnum::BreederClosed => "主动关闭",
        OrderStatusEnum::ConfirmedFinished => "确认完成",
        OrderStatusEnum::ConfirmedUnfinished => "确认未完成",
        OrderStatusEnum::Timeout => "已超时",
        OrderStatusEnum::Cancelled => "已取消",
        OrderStatusEnum::SystemClosed => "系统关闭",
    }
}
