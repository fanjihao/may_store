use crate::{
    errors::CustomError,
    models::{ users::UserToken, wishes::{ WishOut, WishQuery, WishClaimStatusEnum } },
    AppState,
};
use ntex::web::{ types::{ Path, Query, State }, HttpResponse, Responder };
use sqlx::{ QueryBuilder, Row };
use std::sync::Arc;

#[utoipa::path(
    get,
    path = "/wishes",
    tag = "心愿",
    params(WishQuery),
    responses((status = 200, body = [WishOut]))
)]
pub async fn get_wishes(
    _: UserToken,
    state: State<Arc<AppState>>,
    query: Query<WishQuery>
) -> Result<impl Responder, CustomError> {
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT w.wish_id, w.wish_name, w.wish_cost, w.status, w.created_by, w.created_at, w.updated_at, wc.status as claim_status \
         FROM wishes w LEFT JOIN wish_claims wc ON w.wish_id = wc.wish_id"
    );
    
    let mut first = true;
    if query.status.is_some() || query.created_by.is_some() {
        qb.push(" WHERE ");
    }
    if let Some(st) = query.status {
        if !first {
            qb.push(" AND ");
        }
        first = false;
        qb.push(" w.status = ");
        qb.push_bind(st);
    }
    if let Some(cb) = query.created_by {
        if !first {
            qb.push(" AND ");
        }
        qb.push(" w.created_by = ");
        qb.push_bind(cb);
    }
    qb.push(" ORDER BY w.created_at DESC ");
    if let Some(limit) = query.limit {
        qb.push(" LIMIT ");
        qb.push_bind(limit);
    }
    let query_final = qb.build();
    let rows = query_final.fetch_all(&state.db_pool).await?;
    let list: Vec<WishOut> = rows
        .into_iter()
        .map(|r| {
            let claim_status: Option<WishClaimStatusEnum> = r.try_get("claim_status").ok().flatten();
            WishOut {
                wish_id: r.get("wish_id"),
                wish_name: r.get("wish_name"),
                wish_cost: r.get("wish_cost"),
                status: r.get("status"),
                created_by: r.get("created_by"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                current_claim_status: claim_status,
            }
        })
        .collect();
    Ok(HttpResponse::Ok().json(&list))
}

#[utoipa::path(
    get,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn get_wish_detail(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>
) -> Result<impl Responder, CustomError> {
    let row = sqlx
        ::query(
            "SELECT w.wish_id, w.wish_name, w.wish_cost, w.status, w.created_by, w.created_at, w.updated_at, wc.status as claim_status \
         FROM wishes w LEFT JOIN wish_claims wc ON w.wish_id = wc.wish_id AND wc.user_id = $2 \
         WHERE w.wish_id=$1"
        )
        .bind(*id)
        .bind(user_token.user_id)
        .fetch_optional(&state.db_pool).await?;
    let Some(r) = row else {
        return Err(CustomError::BadRequest("心愿不存在".into()));
    };
    let claim_status: Option<WishClaimStatusEnum> = r.try_get("claim_status").ok().flatten();
    let out = WishOut {
        wish_id: r.get("wish_id"),
        wish_name: r.get("wish_name"),
        wish_cost: r.get("wish_cost"),
        status: r.get("status"),
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        current_claim_status: claim_status,
    };
    Ok(HttpResponse::Ok().json(&out))
}
