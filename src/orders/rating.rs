use crate::{
    errors::CustomError,
    models::{ orders::{ OrderRatingCreateInput, OrderRatingOut, OrderStatusEnum }, users::UserToken },
    AppState,
};
use ntex::web::{ types::{ Json, Path, State }, HttpResponse, Responder };
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
    post,
    path = "/orders-rating/{order_id}",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    request_body = OrderRatingCreateInput,
    responses((status = 201, body = OrderRatingOut))
)]
pub async fn create_order_rating(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>,
    body: Json<OrderRatingCreateInput>
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 校验 delta
    if body.delta == 0 || body.delta.abs() > 5 {
        return Err(CustomError::BadRequest("评分增减范围为 -5..5 且不能为0".into()));
    }
    // 获取订单并校验状态、权限
    let order_row = sqlx
        ::query(
            "SELECT
                o.order_id,
                o.user_id,
                o.status,
                agm.user_id as target_user
            FROM
                orders o
            LEFT JOIN 
                association_group_members agm 
            ON 
                agm.group_id = o.group_id
            WHERE
                o.order_id = $1 AND agm.role_in_group = 'RECEIVING' FOR UPDATE"
        )
        .bind(*order_id)
        .fetch_optional(db).await?;
    let Some(or) = order_row else {
        return Err(CustomError::BadRequest("订单不存在".into()));
    };
    let status: OrderStatusEnum = or.get("status");
    if status != OrderStatusEnum::FINISHED {
        return Err(CustomError::BadRequest("仅完成的订单可评分".into()));
    }
    let user_id: i64 = or.get("user_id");
    if user_id != user_token.user_id {
        return Err(CustomError::BadRequest("仅下单用户可评分".into()));
    }
    let receiver_id: i64 = or.get("target_user");

    // 检查是否已有评分
    let existing = sqlx
        ::query("SELECT rating_id FROM order_ratings WHERE order_id=$1")
        .bind(*order_id)
        .fetch_optional(db).await?;
    if existing.is_some() {
        return Err(CustomError::BadRequest("该订单已评分".into()));
    }

    // 开启事务
    let mut tx = db.begin().await?;
    // 锁定接单用户积分
    let target_row = sqlx
        ::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
        .bind(receiver_id)
        .fetch_optional(&mut *tx).await?;
    let Some(target_row) = target_row else {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("被评分用户不存在".into()));
    };
    let current_lp: i32 = target_row.get("love_point");
    let balance_after = current_lp + body.delta; // delta 可为负
    sqlx
        ::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
        .bind(receiver_id)
        .bind(balance_after)
        .execute(&mut *tx).await?;
    // 积分流水
    sqlx
        ::query(
            "INSERT INTO point_transactions (user_id, amount, type, ref_type, ref_id, balance_after) VALUES ($1,$2,'ORDER_RATING',1,$3,$4)"
        )
        .bind(receiver_id)
        .bind(body.delta)
        .bind(*order_id)
        .bind(balance_after)
        .execute(&mut *tx).await?;
    // 插入评分记录
    let rating_row = sqlx
        ::query(
            "INSERT INTO order_ratings (order_id, rater_user_id, target_user_id, delta, remark) VALUES ($1,$2,$3,$4,$5) RETURNING rating_id, order_id, rater_user_id, target_user_id, delta, remark, created_at"
        )
        .bind(*order_id)
        .bind(user_token.user_id)
        .bind(receiver_id)
        .bind(body.delta)
        .bind(&body.remark)
        .fetch_one(&mut *tx).await?;
    tx.commit().await?;
    let out = OrderRatingOut {
        rating_id: rating_row.get("rating_id"),
        order_id: rating_row.get("order_id"),
        rater_user_id: rating_row.get("rater_user_id"),
        target_user_id: rating_row.get("target_user_id"),
        delta: rating_row.get("delta"),
        remark: rating_row.try_get("remark").ok(),
        created_at: rating_row.get("created_at"),
    };
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    get,
    path = "/orders-rating/{order_id}",
    tag = "订单",
    params(("order_id" = i64, Path, description = "订单ID")),
    responses((status = 200, body = OrderRatingOut))
)]
pub async fn get_order_rating(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    order_id: Path<i64>
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 订单存在性与权限（必须为下单用户或接单用户之一）
    let order_row = sqlx
        ::query("SELECT user_id, receiver_id FROM orders WHERE order_id=$1")
        .bind(*order_id)
        .fetch_optional(db).await?;
    let Some(or) = order_row else {
        return Err(CustomError::BadRequest("订单不存在".into()));
    };
    let ouid: i64 = or.get("user_id");
    let rid_opt: Option<i64> = or.try_get("receiver_id").ok();
    if ouid != user_token.user_id && rid_opt != Some(user_token.user_id) {
        return Err(CustomError::BadRequest("无权查看该订单评分".into()));
    }
    let rating_row = sqlx
        ::query(
            "SELECT rating_id, order_id, rater_user_id, target_user_id, delta, remark, created_at FROM order_ratings WHERE order_id=$1"
        )
        .bind(*order_id)
        .fetch_optional(db).await?;
    let Some(rr) = rating_row else {
        return Err(CustomError::BadRequest("该订单尚未评分".into()));
    };
    let out = OrderRatingOut {
        rating_id: rr.get("rating_id"),
        order_id: rr.get("order_id"),
        rater_user_id: rr.get("rater_user_id"),
        target_user_id: rr.get("target_user_id"),
        delta: rr.get("delta"),
        remark: rr.try_get("remark").ok(),
        created_at: rr.get("created_at"),
    };
    Ok(HttpResponse::Ok().json(&out))
}
