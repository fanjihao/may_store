use chrono::{DateTime, Duration, Local, Utc};
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use sqlx::Row;
use sqlx::{Postgres, Transaction};
use std::sync::Arc;
use tokio::time;

use crate::{
    errors::CustomError,
    models::{orders::UpdateOrder, users::UserToken, wx_official::TemplateMessage},
    wx_official::send_to_user::send_template,
    AppState,
};

#[derive(Debug, Copy, Clone, PartialEq)]
enum OrderStatus {
    Pending = 0,
    Accepted = 1,
    Expired = 2,
    Completed = 3,
    Incomplete = 4,
    Rejected = 7,
}

impl OrderStatus {
    fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Pending),
            1 => Some(Self::Accepted),
            2 => Some(Self::Expired),
            3 => Some(Self::Completed),
            4 => Some(Self::Incomplete),
            7 => Some(Self::Rejected),
            _ => None,
        }
    }

    fn message_title(&self) -> String {
        match self {
            Self::Accepted => "您的订单已被接单".to_string(),
            Self::Rejected => "您的订单已被拒绝".to_string(),
            Self::Completed => "您的订单已被标记为完成".to_string(),
            Self::Incomplete => "您的订单已被标记为未完成".to_string(),
            _ => "订单状态已改动".to_string(),
        }
    }

    fn template_status(&self) -> &'static str {
        match self {
            Self::Accepted => "已接单",
            Self::Rejected => "已拒绝",
            Self::Completed => "已完成",
            Self::Incomplete => "未完成",
            _ => "未完成",
        }
    }
}

struct PointsOperation {
    points: i32,
    transaction_type: &'static str,
    description: &'static str,
}

impl PointsOperation {
    fn for_status(status: OrderStatus, points: i32) -> Option<Self> {
        match status {
            OrderStatus::Completed => Some(Self {
                points,
                transaction_type: "earn",
                description: "订单完成获得",
            }),
            OrderStatus::Incomplete => Some(Self {
                points,
                transaction_type: "deduct",
                description: "订单未完成扣除",
            }),
            _ => None,
        }
    }
}

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
    user_token: UserToken,
    data: Json<UpdateOrder>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut transaction = db_pool.begin().await?;

    let (order_no, food_names) = tokio::try_join!(
        sqlx::query_scalar!(
            "SELECT order_no FROM orders WHERE order_id = $1",
            data.id
        )
        .fetch_one(db_pool),
        sqlx::query_scalar!(
            "SELECT string_agg(f.food_name, ', ') 
            FROM orders_d d 
            LEFT JOIN foods f ON d.food_id = f.food_id 
            WHERE d.order_id = $1
            GROUP BY d.order_id",
            data.id
        )
        .fetch_one(db_pool)
    )?;

    // 获取订单状态
    let status = OrderStatus::from_i32(data.status)
        .ok_or_else(|| CustomError::BadRequest("Invalid order status".into()))?;
    
    // 更新订单状态
    match status {
        OrderStatus::Accepted | OrderStatus::Rejected => {
            let approval_status = if matches!(status, OrderStatus::Accepted) { 1 } else { 0 };
            sqlx::query!(
                "UPDATE orders SET order_status = $1, approval_feedback = $3, approval_time = $4, approval_status = $5 WHERE order_id = $2",
                data.status,
                data.id,
                data.approval_feedback,
                Local::now(),
                approval_status
            )
            .execute(&mut *transaction)
            .await?;
        
            let desc = if data.status == 1 {
                "接受了"
            } else {
                "拒绝了"
            };
            let actions = format!("xxx {} 订单{} [ {} ]", desc, order_no.clone().unwrap_or_default(), food_names.clone().unwrap_or_default());
            insert_footprints(
                &mut transaction,
                data.ship_id.unwrap(),
                &food_names.clone().unwrap_or_default(),
                &order_no.clone().unwrap_or_default(),
                &actions
            )
            .await?;
        },
        OrderStatus::Completed | OrderStatus::Incomplete => {
            let finish_status = if matches!(status, OrderStatus::Completed) { 1 } else { 0 };
            
            // 更新订单状态
            sqlx::query!(
                "UPDATE orders SET order_status = $1, finish_feedback = $3, finish_time = $4, finish_status = $5 WHERE order_id = $2",
                data.status,
                data.id,
                data.finish_feedback,
                Local::now(),
                finish_status
            )
            .execute(&mut *transaction)
            .await?;

            let desc = if data.status == 3 {
                "完成了"
            } else {
                "未完成"
            };
            let actions = format!("xxx 将订单{} [ {} ] 标记为 {}", order_no.clone().unwrap_or_default(), food_names.clone().unwrap_or_default(), desc);
            insert_footprints(
                &mut transaction,
                data.ship_id.unwrap(),
                &food_names.clone().unwrap_or_default(),
                &order_no.clone().unwrap_or_default(),
                &actions
            )
            .await?;

            // 处理积分
            if let (Some(user_id), Some(points)) = (data.user_id, data.points) {
                let row = sqlx::query!(
                    "SELECT love_point FROM users WHERE user_id = $1",
                    user_id
                )
                .fetch_one(&mut *transaction)
                .await?;

                let current_points = row.love_point.unwrap_or(0);
                let balance = match status {
                    OrderStatus::Completed => current_points + points,
                    _ => current_points - points
                };

                if let Some(points_op) = PointsOperation::for_status(status, points) {
                    log_points_transaction(
                        &mut transaction,
                        user_id,
                        points,
                        points_op.transaction_type,
                        balance,
                        points_op.description,
                        data.id,
                    )
                    .await?;
                }
            }
        },
        _ => {
            let actions = format!("xxx 撤回了 订单{} [ {} ]", order_no.clone().unwrap_or_default(), food_names.clone().unwrap_or_default());
            insert_footprints(
                &mut transaction,
                data.ship_id.unwrap(),
                &food_names.clone().unwrap_or_default(),
                &order_no.clone().unwrap_or_default(),
                &actions
            )
            .await?;

            sqlx::query!(
                "UPDATE orders SET order_status = $1, revoke_time = $3 WHERE order_id = $2",
                data.status,
                data.id,
                Local::now()
            )
            .execute(&mut *transaction)
            .await?;
        }
    }

    transaction.commit().await?;

    // 发送通知
    if let Some(user_info) = user_token.user_info {
        if let Some(push_id) = user_info.push_id {
            // 发送通知
            let template_id = sqlx::query_scalar!(
                "SELECT template_id FROM templates WHERE types = 'orders'"
            )
            .fetch_one(db_pool)
            .await?;

            send_template(Json(TemplateMessage {
                template_id,
                push_id: push_id.to_string(),
                msg_title: status.message_title(),
                order_no: order_no.expect("REASON"),
                date_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                foods: food_names.unwrap_or_default(),
                order_status: status.template_status().to_string(),
            }))
            .await?;
        }
    }

    Ok(HttpResponse::Created().body("操作成功"))
}

pub async fn insert_footprints(
    transaction: &mut Transaction<'_, Postgres>,
    ship_id: i32,
    foods: &str,
    order_no: &str,
    actions: &str,
) -> Result<(), CustomError> {
    sqlx::query!(
        "INSERT INTO footprints (ship_id, foods, order_no, action) VALUES ($1, $2, $3, $4)",
        ship_id,
        foods,
        order_no,
        actions
    )
    .execute(&mut **transaction)
    .await?;

    Ok(())
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
        let _ = process_expired_orders(expired_orders, &state).await;

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

async fn process_expired_orders(
    expired_orders: Vec<ExpiredOrder>,
    state: &Arc<AppState>,
) -> Result<(), CustomError> {
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

            let query = format!("SELECT food_name FROM orders_d d LEFT JOIN foods f ON d.food_id = f.food_id WHERE d.order_id = ANY($1)");

            let result: Vec<String> = sqlx::query(&query)
                .bind(&ids)
                .fetch_all(db_pool)
                .await?
                .into_iter()
                .map(|row| row.get("food_name"))
                .collect();

            let food_names = result.join(", ");

            let _ = send_template(Json(TemplateMessage {
                template_id: tp_record.template_id.clone(),
                push_id: record.push_id.expect("no push id"),
                msg_title: format!("您的订单时间太长未接单！"),
                order_no: value.order_no.clone().expect("REASON"),
                date_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                foods: food_names,
                order_status: "已过期".to_string(),
            }))
            .await;
        }
    }
    Ok(())
}
