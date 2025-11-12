use chrono::Utc;
use reqwest::Client;
use sqlx::Row;
use sqlx::postgres::PgPool;

use crate::{
    errors::CustomError,
    models::{orders::OrderStatusEnum, wx_official::TemplateMessage},
    utils::ORDER_TEMPLATE_ID,
};
use crate::wx_official::auth::{fetch_set_access_token, get_access_token};

// 推送订单状态变更（根据 order_id 查询订单、菜品、用户 push_id 并发送模板消息）
// 失败时只记录日志，不影响主流程。
pub async fn push_order_status(order_id: i64, db_pool: PgPool) -> Result<(), CustomError> {
    // 查询订单 + 相关用户 push_id
    let order_row = sqlx::query(
        "SELECT order_id, user_id, receiver_id, status FROM orders WHERE order_id=$1"
    )
        .bind(order_id)
        .fetch_optional(&db_pool)
        .await?;
    let row = match order_row { Some(r) => r, None => return Ok(()), };
    let status_str: String = row.get("status");
    let status = match status_str.as_str() {
        "PENDING" => OrderStatusEnum::PENDING,
        "ACCEPTED" => OrderStatusEnum::ACCEPTED,
        "FINISHED" => OrderStatusEnum::FINISHED,
        "CANCELLED" => OrderStatusEnum::CANCELLED,
        "EXPIRED" => OrderStatusEnum::EXPIRED,
        "REJECTED" => OrderStatusEnum::REJECTED,
        "SYSTEM_CLOSED" => OrderStatusEnum::SYSTEM_CLOSED,
        _ => OrderStatusEnum::PENDING,
    };

    let user_id: i64 = row.get("user_id");
    let receiver_id: Option<i64> = row.try_get("receiver_id").ok();

    // 聚合菜品名称（最多取5个）
    let food_rows = sqlx::query(
        "SELECT f.food_name FROM order_items oi JOIN foods f ON oi.food_id=f.food_id WHERE oi.order_id=$1 LIMIT 5"
    )
        .bind(order_id)
        .fetch_all(&db_pool)
        .await?;
    let mut names: Vec<String> = Vec::new();
    for fr in food_rows { names.push(fr.get::<String, _>("food_name")); }
    let foods_summary = if names.is_empty() { "-".to_string() } else { names.join(" / ") };

    // 获取 push_id（下单人 + 接单人）
    let mut push_ids: Vec<String> = Vec::new();
    if let Some(pid) = fetch_push_id(user_id, &db_pool).await? { push_ids.push(pid); }
    if let Some(rid) = receiver_id { if let Some(pid) = fetch_push_id(rid, &db_pool).await? { push_ids.push(pid); } }
    if push_ids.is_empty() { return Ok(()); }

    // 获取 access_token
    fetch_set_access_token().await?;
    let token_opt = get_access_token().await;
    let Some(access_token) = token_opt else { return Ok(()); };

    let client = Client::new();
    let status_cn = status_to_cn(status);
    let now_str = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    for pid in push_ids {
        let msg = TemplateMessage {
            template_id: ORDER_TEMPLATE_ID.to_string(),
            push_id: pid.clone(),
            msg_title: "订单状态更新".to_string(),
            order_no: order_id.to_string(),
            date_time: now_str.clone(),
            foods: foods_summary.clone(),
            order_status: status_cn.to_string(),
        };
        // 发送
        let json_data = serde_json::json!({
            "touser": msg.push_id,
            "template_id": msg.template_id,
            "url": "http://weixin.qq.com/download",
            "topcolor": "#FF0000",
            "data": {
                "msg_title": {"value": msg.msg_title, "color": "#173177" },
                "order_no": {"value": msg.order_no, "color": "#173177" },
                "date_time": {"value": msg.date_time, "color": "#173177" },
                "foods": {"value": msg.foods, "color": "#173177" },
                "order_status": {"value": msg.order_status, "color": "#173177" }
            }
        });
        if let Err(e) = client.post(
            format!("https://api.weixin.qq.com/cgi-bin/message/template/send?access_token={}", access_token)
        ).json(&json_data).send().await { log::warn!("push order status send error: {}", e); }
    }

    Ok(())
}

async fn fetch_push_id(user_id: i64, db_pool: &PgPool) -> Result<Option<String>, CustomError> {
    let row = sqlx::query("SELECT push_id FROM users WHERE user_id=$1")
        .bind(user_id)
        .fetch_optional(db_pool)
        .await?;
    Ok(row.and_then(|r| r.try_get::<Option<String>, _>("push_id").ok()).flatten())
}

fn status_to_cn(s: OrderStatusEnum) -> &'static str {
    match s {
        OrderStatusEnum::PENDING => "待处理",
        OrderStatusEnum::ACCEPTED => "已接单",
        OrderStatusEnum::FINISHED => "已完成",
        OrderStatusEnum::CANCELLED => "已取消",
        OrderStatusEnum::EXPIRED => "已过期",
        OrderStatusEnum::REJECTED => "已拒绝",
        OrderStatusEnum::SYSTEM_CLOSED => "系统关闭",
    }
}
