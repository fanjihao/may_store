use std::sync::Arc;

use ntex::web::{types::{Json, State}, HttpResponse, Responder};
use sqlx::{Postgres, Transaction};

use crate::{errors::CustomError, models::orders::UpdateOrder, AppState};

pub async fn update_order(
    data: Json<UpdateOrder>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut transaction = db_pool.begin().await?;

    if data.status == 7 || data.status == 1 { // 接单或拒绝
        sqlx::query!(
            "UPDATE orders SET order_status = $1, approval_feedback = $3, approval_time = $4 WHERE order_id = $2",
            data.status,
            data.id,
            data.approval_feedback,
            chrono::Local::now()
        )
        .execute(&mut *transaction)
        .await?;
    } else if data.status == 3 || data.status == 4 { // 完成或未完成
        let row = sqlx::query!(
            "SELECT love_point FROM users WHERE user_id = $1",
            data.user_id
        )
        .fetch_one(&mut *transaction)
        .await?;

        sqlx::query!(
            "UPDATE orders SET order_status = $1, finish_feedback = $3, finish_time = $4 WHERE order_id = $2",
            data.status,
            data.id,
            data.finish_feedback,
            chrono::Local::now()
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
            description
        ).await?;

    } else {
        sqlx::query!(
            "UPDATE orders SET order_status = $1 WHERE order_id = $2",
            data.status,
            data.id
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
) -> Result<(), CustomError> {

    sqlx::query!(
        "INSERT INTO points_history (user_id, points, transaction_type, balance, description) VALUES ($1, $2, $3, $4, $5)",
        user_id,
        points,
        transaction_type,
        balance,
        description
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
