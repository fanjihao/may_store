use crate::{
    errors::CustomError,
    models::{ users::UserToken, wishes::{ WishOut, WishRecord, WishStatusEnum, WishUpdateInput } },
    AppState,
};
use ntex::web::{ types::{ Json, State }, HttpResponse, Responder };
use sqlx::QueryBuilder;
use sqlx::Row;
use std::sync::Arc;

#[utoipa::path(
    put,
    path = "/wishes",
    tag = "心愿",
    request_body = WishUpdateInput,
    responses((status = 200, body = WishOut))
)]
pub async fn update_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<WishUpdateInput>
) -> Result<impl Responder, CustomError> {
    if data.wish_name.is_none() && data.wish_cost.is_none() && data.status.is_none() {
        return Err(CustomError::BadRequest("无修改内容".into()));
    }
    let db = &state.db_pool;
    let row = sqlx
        ::query(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, created_at, updated_at FROM wishes WHERE wish_id=$1"
        )
        .bind(data.wish_id)
        .fetch_optional(db).await?;
    let Some(r) = row else {
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    let created_by: i64 = r.get("created_by");
    if created_by != user_token.user_id {
        return Err(CustomError::BadRequest("只能修改自己创建的心愿".into()));
    }
    // Build dynamic update
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("UPDATE wishes SET ");
    let mut first = true;
    if let Some(name) = &data.wish_name {
        if !first {
            qb.push(", ");
        }
        first = false;
        qb.push(" wish_name = ").push_bind(name);
    }
    if let Some(cost) = data.wish_cost {
        if !first {
            qb.push(", ");
        }
        first = false;
        qb.push(" wish_cost = ").push_bind(cost);
    }
    if let Some(st) = data.status {
        if !first {
            qb.push(", ");
        }
        first = false;
        qb.push(" status = ").push_bind(st);
    }
    if first {
        return Err(CustomError::BadRequest("无修改内容".into()));
    }
    qb.push(", updated_at = NOW() WHERE wish_id = ")
        .push_bind(data.wish_id)
        .push(
            " RETURNING wish_id, wish_name, wish_cost, status, created_by, created_at, updated_at"
        );
    let updated = qb.build().fetch_one(db).await?;
    let rec = WishRecord {
        wish_id: updated.get("wish_id"),
        wish_name: updated.get("wish_name"),
        wish_cost: updated.get("wish_cost"),
        status: updated.get("status"),
        created_by,
        created_at: updated.get("created_at"),
        updated_at: updated.get("updated_at"),
    };
    Ok(HttpResponse::Ok().json(&WishOut::from(rec)))
}

#[utoipa::path(
    delete,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn disable_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: ntex::web::types::Path<i64>
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let row = sqlx
        ::query(
            "SELECT wish_id, wish_name, wish_cost, status, created_by, created_at, updated_at FROM wishes WHERE wish_id=$1"
        )
        .bind(*id)
        .fetch_optional(db).await?;
    let Some(r) = row else {
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    let created_by: i64 = r.get("created_by");
    if created_by != user_token.user_id {
        return Err(CustomError::BadRequest("只能关闭自己创建的心愿".into()));
    }
    sqlx
        ::query("UPDATE wishes SET status='OFF', updated_at=NOW() WHERE wish_id=$1")
        .bind(*id)
        .execute(db).await?;
    let rec = WishRecord {
        wish_id: r.get("wish_id"),
        wish_name: r.get("wish_name"),
        wish_cost: r.get("wish_cost"),
        status: WishStatusEnum::OFF,
        created_by,
        created_at: r.get("created_at"),
        updated_at: chrono::Utc::now(),
    };
    Ok(HttpResponse::Ok().json(&WishOut::from(rec)))
}
