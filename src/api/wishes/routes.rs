// API 层 - 心愿路由
// 处理心愿相关的 HTTP 请求

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use std::sync::Arc;

use crate::application::wish_service::WishService;
use crate::domain::wish::{
    WishCreateInput, WishCursor, WishDeadlineInput, WishFeedbackInput, WishOut, WishQuery,
    WishQuoteInput, WishRejectInput, WishUpdateInput,
};
use crate::{
    config::AppState,
    errors::CustomError,
    middlewares::auth::UserToken,
    models::pagination::{decode_cursor, encode_cursor, CursorPage},
};

/// 配置心愿路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/wishes")
            .route("", web::get().to(list_wishes))
            .route("/{id}", web::get().to(get_wish))
            // FSD v2: 心愿协商与选择接口
            .route("/{id}/quote", web::post().to(wish_quote))
            .route("/{id}/deadline", web::post().to(wish_deadline))
            .route(
                "/{id}/confirm-agreement",
                web::post().to(wish_confirm_agreement),
            )
            .route("/{id}/reject", web::post().to(wish_reject))
            .route("/{id}/select", web::post().to(wish_select))
            .route("/{id}/feedback", web::put().to(submit_feedback)),
    );
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

// ============== FSD v2 心愿协商与选择接口 ==============

/// 协商报价 - 发起人或履约人报价或还价
/// POST /api/wishes/{id}/quote
#[utoipa::path(
    post,
    path = "/wishes/{id}/quote",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishQuoteInput,
    responses(
        (status = 200, description = "报价成功"),
        (status = 400, description = "心愿状态不允许报价"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_quote(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishQuoteInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out = WishService::quote_wish(&state.db_pool, user_token.user_id, wish_id, &input).await?;
    Ok(HttpResponse::Ok().json(&out))
}

/// 协商履约期限 - 发起人或履约人设置履约期限
/// POST /api/wishes/{id}/deadline
#[utoipa::path(
    post,
    path = "/wishes/{id}/deadline",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishDeadlineInput,
    responses(
        (status = 200, description = "设置成功"),
        (status = 400, description = "心愿状态不允许设置期限"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_deadline(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishDeadlineInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out =
        WishService::set_deadline(&state.db_pool, user_token.user_id, wish_id, &input).await?;
    Ok(HttpResponse::Ok().json(&out))
}

/// 双方线上确认积分和期限 - 心愿进入 CREATED 状态
/// POST /api/wishes/{id}/confirm-agreement
#[utoipa::path(
    post,
    path = "/wishes/{id}/confirm-agreement",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "确认成功，心愿进入心愿池"),
        (status = 400, description = "心愿状态不允许确认"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_confirm_agreement(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let out = WishService::confirm_agreement(&state.db_pool, user_token.user_id, wish_id).await?;
    Ok(HttpResponse::Ok().json(&out))
}

/// 拒绝或关闭协商
/// POST /api/wishes/{id}/reject
#[utoipa::path(
    post,
    path = "/wishes/{id}/reject",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishRejectInput,
    responses(
        (status = 200, description = "操作成功"),
        (status = 400, description = "心愿状态不允许此操作"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_reject(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishRejectInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out = WishService::reject_wish(&state.db_pool, user_token.user_id, wish_id, &input).await?;
    Ok(HttpResponse::Ok().json(&out))
}

/// 选择心愿并冻结积分
/// POST /api/wishes/{id}/select
///
/// 发起人选择心愿，冻结其爱心积分
#[utoipa::path(
    post,
    path = "/wishes/{id}/select",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "选择成功，积分已冻结"),
        (status = 400, description = "积分不足或心愿状态不允许"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_select(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let out = WishService::select_wish(&state.db_pool, user_token.user_id, wish_id).await?;
    Ok(HttpResponse::Ok().json(&out))
}
