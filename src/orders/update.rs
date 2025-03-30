use std::sync::Arc;

use chrono::{DateTime, Duration, Local, Utc};
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use sqlx::{Postgres, Transaction};
use tokio::time;

use crate::{
    errors::CustomError,
    models::{orders::UpdateOrder, wx_official::TemplateMessage},
    wx_official::send_to_user::send_template,
    AppState,
};

#[utoipa::path(
    put,
    path = "/orders",
    tag = "订单",
    request_body = UpdateOrder,
    responses(
        (status = 200, body = String, description = "操作成功")
    )
)]
pub async fn update_order(
    data: Json<UpdateOrder>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut transaction = db_pool.begin().await?;

    let order_record = sqlx::query!("SELECT * FROM orders o WHERE o.order_id = $1", data.id)
        .fetch_one(&mut *transaction)
        .await?;
    let push_user = data.user_id.unwrap();
    let mut msg_title = String::new();
    if data.status == 7 || data.status == 1 {
        msg_title = format!("您的订单已被拒绝");
        // 接单或拒绝
        let mut status = 0;
        if data.status == 1 {
            status = 1;
            msg_title = format!("您的订单已被接单");
        }
        sqlx::query!(
            "UPDATE orders SET order_status = $1, approval_feedback = $3, approval_time = $4, approval_status = $5 WHERE order_id = $2",
            data.status,
            data.id,
            data.approval_feedback,
            chrono::Local::now(),
            status
        )
        .execute(&mut *transaction)
        .await?;
    } else if data.status == 3 || data.status == 4 {
        // 完成或未完成
        let mut status = 0;
        msg_title = format!("您的订单已被标记为未完成");
        if data.status == 3 {
            status = 1;
            msg_title = format!("您的订单已被标记为完成");
        }
        let row = sqlx::query!(
            "SELECT love_point FROM users WHERE user_id = $1",
            data.user_id
        )
        .fetch_one(&mut *transaction)
        .await?;

        sqlx::query!(
            "UPDATE orders SET order_status = $1, finish_feedback = $3, finish_time = $4, finish_status = $5 WHERE order_id = $2",
            data.status,
            data.id,
            data.finish_feedback,
            chrono::Local::now(),
            status
        )
        .execute(&mut *transaction)
        .await?;

        let balance = row.love_point.unwrap_or(0) + data.points.unwrap();
        let mut transaction_type = "earn";
        if data.status == 4 {
            transaction_type = "deduct";
        }

        let mut description = "订单完成获得";
        if data.status == 4 {
            description = "订单未完成扣除";
        }

        sqlx::query!(
            "UPDATE users SET love_point = $1 WHERE user_id = $2",
            balance,
            data.user_id,
        )
        .execute(&mut *transaction)
        .await?;

        log_points_transaction(
            &mut transaction,
            data.user_id.unwrap(),
            data.points.unwrap(),
            transaction_type,
            balance,
            description,
            data.id,
        )
        .await?;
    } else {
        msg_title = "订单状态已改动".to_string();
        sqlx::query!(
            "UPDATE orders SET order_status = $1, revoke_time = $3 WHERE order_id = $2",
            data.status,
            data.id,
            chrono::Local::now()
        )
        .execute(&mut *transaction)
        .await?;
    }
    
    let tp_record = sqlx::query!("SELECT * FROM templates WHERE templates.types = 'orders'")
        .fetch_one(&mut *transaction)
        .await?;
    let record = sqlx::query!(
        "SELECT * FROM users WHERE user_id = $1",
        &push_user
    )
    .fetch_one(&mut *transaction)
    .await
    .unwrap();

    let tp_status;
    if data.status == 1 {
        tp_status = "已接单";
    } else if data.status == 7 {
        tp_status = "已拒绝";
    } else if data.status == 3 {
        tp_status = "已完成";
    } else if data.status == 4 {
        tp_status = "未完成";
    } else {
        tp_status = "未完成";
    }
    let _ = send_template(Json(TemplateMessage {
        template_id: tp_record.template_id.clone(),
        push_id: record.push_id.expect("no push id"),
        new_order: msg_title,
        order_no: order_record.order_no.clone().expect("REASON"),
        date_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        foods: "".to_string(),
        order_status: tp_status.to_string(),
    }))
    .await;
    transaction.commit().await?;
    Ok(HttpResponse::Created().body("操作成功"))
}

pub async fn log_points_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i32,
    points: i32,
    transaction_type: &str,
    balance: i32,
    description: &str,
    bind_id: i32,
) -> Result<(), CustomError> {
    sqlx::query!(
        "INSERT INTO points_history (user_id, points, transaction_type, balance, description, bind_id) VALUES ($1, $2, $3, $4, $5, $6)",
        user_id,
        points,
        transaction_type,
        balance,
        description,
        bind_id
    )
    .execute(&mut **transaction)
    .await?;

    sqlx::query!(
        "UPDATE users SET love_point = $2 WHERE user_id = $1",
        user_id,
        balance,
    )
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

// 定时任务用来查询失效订单的
pub async fn check_order_expiration(state: Arc<AppState>) {
    loop {
        let now = Local::now();
        // 查询数据库，找出已经过期的订单
        let expired_orders = query_expired_orders(now.into(), &state).await;

        // 处理过期订单，例如取消订单、释放库存、通知用户等
        process_expired_orders(expired_orders, &state).await;

        // // 等待一段时间后再次检查
        let sleep_duration = Duration::minutes(1).to_std().unwrap();
        time::sleep(sleep_duration).await;
    }
}
struct ExpiredOrder {
    order_id: i32,
    order_no: Option<String>,
    user_id: Vec<i32>,
}
async fn query_expired_orders(now: DateTime<Utc>, state: &Arc<AppState>) -> Vec<ExpiredOrder> {
    let db_pool = &state.clone().db_pool;
    let expiration_threshold = now - Duration::minutes(30); // 假设失效时间为30分钟前

    // 执行数据库查询，找出已经过期的订单
    let expired_orders = sqlx::query!(
        "SELECT * FROM orders WHERE create_date < $1 and order_status = 0 and order_status != 2",
        expiration_threshold
    )
    .fetch_all(db_pool)
    .await
    .unwrap()
    .iter()
    .map(|i| ExpiredOrder {
        order_id: i.order_id,
        order_no: i.order_no.clone(),
        user_id: [i.recv_user_id, i.create_user_id]
            .into_iter()
            .flatten()
            .collect(),
    })
    .collect();

    return expired_orders;
}

async fn process_expired_orders(expired_orders: Vec<ExpiredOrder>, state: &Arc<AppState>) {
    if expired_orders.len() > 0 {
        let db_pool = &state.clone().db_pool;

        let mut ids = Vec::new();
        for value in &expired_orders[0..] {
            ids.push(value.order_id);
        }
        // 使用一个SQL语句批量更新订单状态
        sqlx::query!(
            "UPDATE orders SET order_status = 2 WHERE order_id = ANY($1)",
            &ids
        )
        .execute(db_pool)
        .await
        .expect("处理过期订单失败");

        let tp_record = sqlx::query!("SELECT * FROM templates WHERE templates.types = 'orders'")
            .fetch_one(db_pool)
            .await
            .unwrap();

        for value in &expired_orders[0..] {
            // 在这里执行其他操作，例如释放库存、通知用户等
            let record = sqlx::query!(
                "SELECT * FROM users WHERE user_id = ANY($1)",
                &value.user_id
            )
            .fetch_one(db_pool)
            .await
            .unwrap();

            let _ = send_template(Json(TemplateMessage {
                template_id: tp_record.template_id.clone(),
                push_id: record.push_id.expect("no push id"),
                new_order: "您的订单时间太长未接单！".to_string(),
                order_no: value.order_no.clone().expect("REASON"),
                date_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                foods: "".to_string(),
                order_status: "已过期".to_string(),
            }))
            .await;
        }
    }
}
