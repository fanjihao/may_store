use std::sync::Arc;

use chrono::Local;
use ntex::web::{
    types::{Json, State},
    HttpResponse,
    Responder,
};
use sqlx::Row;
use crate::{errors::CustomError, models::{orders::OrderDto, users:: UserToken, wx_official::TemplateMessage}, wx_official::send_to_user::send_template, AppState};

#[utoipa::path(
    post,
    path = "/orders",
    request_body = OrderDto,
    tag = "订单",
    responses(
        (status = 200, body = String, description = "创建成功")
    )
)]
pub async fn create_order(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<OrderDto>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut transaction = db_pool.begin().await?;

    let order_no = idgenerator::IdInstance::next_id().to_string();
    // 存入order
    let result = sqlx::query!(
        "INSERT INTO orders (order_no, create_user_id, goal_time, remarks, recv_user_id) VALUES ($1, $2, $3, $4, $5) RETURNING order_id",
        order_no,
        data.user_id,
        data.goal_time,
        data.remarks,
        data.recv_id
    )
    .fetch_one(&mut *transaction)
    .await?;

    // 订单传入food ids集合，将他们分开保存
    let food_ids = data.order_child.split(',').collect::<Vec<&str>>();

    // 将 food_ids 转换为一个 Vec<i32>
    let food_ids: Result<Vec<i32>, _> = food_ids.iter().map(|&id| id.parse::<i32>()).collect();
    let food_ids = food_ids?;

    let values: Vec<(i32, i32, i32)> = food_ids
        .iter()
        .map(|&id| (result.order_id, id, data.user_id))
        .collect();

    // 构建 SQL 查询字符串
    let mut query = String::new();
    query.push_str("INSERT INTO orders_d (order_id, food_id, user_id) VALUES ");

    // 如果还有其他值，将它们拼接到查询字符串中
    if values.len() > 0 {
        for value in &values[0..] {
            query.push_str(&format!("({}, {}, {}),", value.0, value.1, value.2));
        }
    }
    let query = query.trim_end_matches(",");

    // 执行查询
    sqlx::query(&query).execute(&mut *transaction).await?;
    transaction.commit().await?;

    if !user_token.user_info.clone().unwrap().push_id.is_none() {
        let tp_record = sqlx::query!(
            "SELECT * FROM templates WHERE templates.types = 'orders'"
        ).fetch_one(db_pool).await?;

        let query = format!(
            "SELECT food_name FROM foods WHERE food_id = ANY($1)"
        );
    
        let result: Vec<String> = sqlx::query(&query)
            .bind(food_ids)  // 将所有 food_ids 绑定到查询中
            .fetch_all(db_pool)
            .await?
            .into_iter()
            .map(|row| row.get("food_name"))
            .collect();
        
        let food_names = result.join(", ");
    
        let _ = send_template(Json(TemplateMessage {
            template_id: tp_record.template_id,
            push_id: user_token.user_info.clone().unwrap().push_id.expect("no push id"),
            msg_title: format!("您有一条新订单！"),
            order_no: order_no.clone(),
            date_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            foods: food_names,
            order_status: "待接单".to_string(),
        })).await;
    }

    Ok(HttpResponse::Created().body("创建成功"))
}
