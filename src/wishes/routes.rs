use super::models::{WishCreateInput, WishFeedbackInput, WishOut, WishQuery, WishUpdateInput};
use super::service::WishService;
use crate::{
    config::AppState,
    errors::CustomError,
    middlewares::auth::UserToken,
    models::pagination::{decode_cursor, encode_cursor, CursorPage},
};
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize)]
pub struct WishCursor {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub wish_id: i64,
}

#[utoipa::path(
    get,
    path = "/wishes",
    tag = "心愿",
    params(WishQuery),
    responses((status = 200, body = CursorPage<WishOut>))
)]
pub async fn list_wishes(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<WishQuery>,
) -> Result<impl Responder, CustomError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let Some(group_id) = query.group_id else {
        return Err(CustomError::BadRequest("Missing group_id".into()));
    };

    let cursor_condition = if let Some(cursor_str) = &query.cursor {
        if let Some(cursor) = decode_cursor::<WishCursor>(cursor_str) {
            Some((cursor.created_at, cursor.wish_id))
        } else {
            None
        }
    } else {
        None
    };

    let (mut rows, total) = WishService::list_wishes(
        &state.db_pool,
        group_id,
        user_token.user_id,
        limit as i64,
        cursor_condition,
    )
    .await?;

    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&WishCursor {
                created_at: r.created_at,
                wish_id: r.wish_id,
            })
        })
    } else {
        None
    };

    let items: Vec<WishOut> = rows
        .into_iter()
        .map(|r| WishOut::from_record(r, None))
        .collect();

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
        total: Some(total),
    }))
}

#[utoipa::path(
    post,
    path = "/wishes",
    tag = "心愿",
    request_body = WishCreateInput,
    responses((status = 201, body = WishOut))
)]
pub async fn create_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<WishCreateInput>,
) -> Result<impl Responder, CustomError> {
    let rec =
        WishService::create_wish(&state.db_pool, user_token.user_id, &data.into_inner()).await?;
    Ok(HttpResponse::Created().json(&WishOut::from_record(rec, None)))
}

#[utoipa::path(
    get,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn get_wish(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let (rec, feedback) = WishService::get_wish(&state.db_pool, *id).await?;
    Ok(HttpResponse::Ok().json(&WishOut::from_record(rec, feedback)))
}

#[utoipa::path(
    put,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishUpdateInput,
    responses((status = 200, body = WishOut))
)]
pub async fn update_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<WishUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let (updated, feedback) =
        WishService::update_wish(&state.db_pool, user_token.user_id, *id, &data.into_inner())
            .await?;
    Ok(HttpResponse::Ok().json(&WishOut::from_record(updated, feedback)))
}

#[utoipa::path(
    delete,
    path = "/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn delete_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let rec = WishService::delete_wish(&state.db_pool, user_token.user_id, *id).await?;
    Ok(HttpResponse::Ok().json(&WishOut::from_record(rec, None)))
}

#[utoipa::path(
    post,
    path = "/wishes/{id}/redeem",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses((status = 200, body = WishOut))
)]
pub async fn redeem_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let rec = WishService::redeem_wish(&state.db_pool, user_token.user_id, *id).await?;
    Ok(HttpResponse::Ok().json(&WishOut::from_record(rec, None)))
}

#[utoipa::path(
    put,
    path = "/wishes/{id}/feedback",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishFeedbackInput,
    responses((status = 200, body = WishOut))
)]
pub async fn submit_feedback(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<WishFeedbackInput>,
) -> Result<impl Responder, CustomError> {
    let (rec, feedback) =
        WishService::submit_feedback(&state.db_pool, user_token.user_id, *id, &data.into_inner())
            .await?;
    Ok(HttpResponse::Ok().json(&WishOut::from_record(rec, feedback)))
}
