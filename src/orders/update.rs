use std::sync::Arc;

use chrono::{DateTime, Duration, Local, Utc};
use ntex::web::{types::{Json, State}, HttpResponse, Responder};
use sqlx::{Postgres, Transaction};
use tokio::time;

use crate::{errors::CustomError, models::orders::UpdateOrder, AppState};

pub async fn update_order(
    data: Json<UpdateOrder>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut transaction = db_pool.begin().await?;

    if data.status == 7 || data.status == 1 { // 接单或拒绝
        let mut status = 0;
        if data.status == 1 {
            status = 1
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
    } else if data.status == 3 || data.status == 4 { // 完成或未完成
        let mut status = 0;
        if data.status == 3 {
            status = 1
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
            data.id
        ).await?;

    } else {
        sqlx::query!(
            "UPDATE orders SET order_status = $1, revoke_time = $3 WHERE order_id = $2",
            data.status,
            data.id,
            chrono::Local::now()
        )
        .execute(&mut *transaction)
        .await?;
    }

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

async fn query_expired_orders(now: DateTime<Utc>, state: &Arc<AppState>) -> Vec<i32> {
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
    .map(|i| i.order_id)
    .collect();

    return expired_orders;
}

async fn process_expired_orders(expired_orders: Vec<i32>, state: &Arc<AppState>) {
    if expired_orders.len() > 0 {
        let db_pool = &state.clone().db_pool;

        // 使用一个SQL语句批量更新订单状态
        sqlx::query!(
            "UPDATE orders SET order_status = 2 WHERE order_id = ANY($1)",
            &expired_orders
        )
        .execute(db_pool)
        .await
        .expect("处理过期订单失败");

        // 在这里执行其他操作，例如释放库存、通知用户等
        // ...

        println!("Updated statuses for expired orders: {:?}", expired_orders);
    }
}