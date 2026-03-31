use super::models::{MemorialDay, MemorialDayCreate, MemorialDayCursor, MemorialDayQuery, MemorialDayUpdate};
use super::service::MemorialDayService;
use crate::config::AppState;
use crate::errors::CustomError;
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse,
};
use std::sync::Arc;

/// 获取纪念日列表
#[utoipa::path(
    get,
    path = "/couple-space/memorial-days",
    tag = "情侣空间",
    params(MemorialDayQuery),
    responses(
        (status = 200, description = "获取成功", body = CursorPage<MemorialDay>),
        (status = 401, description = "未登录", body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn list_memorial_days(
    state: State<Arc<AppState>>,
    query: Query<MemorialDayQuery>,
) -> Result<HttpResponse, CustomError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let cursor_condition = if let Some(cursor_str) = &query.cursor {
        if cursor_str.is_empty() || cursor_str == "null" {
            None
        } else if let Some(cursor) = decode_cursor::<MemorialDayCursor>(cursor_str) {
            Some((cursor.memorial_date, cursor.id))
        } else {
            return Err(CustomError::bad_request("无效的游标"));
        }
    } else {
        None
    };

    let (mut records, total) = MemorialDayService::list_memorial_days(
        &state.db_pool,
        query.group_id,
        limit + 1,
        cursor_condition,
    )
    .await?;

    let has_more = records.len() > limit as usize;
    if has_more {
        records.pop();
    }

    let next_cursor = if has_more {
        records.last().map(|r| {
            encode_cursor(&MemorialDayCursor {
                memorial_date: r.memorial_date,
                id: r.id,
            })
        })
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(&CursorPage {
        items: records,
        next_cursor,
        has_more,
        total: Some(total),
    }))
}

// 获取默认纪念日
#[utoipa::path(
    get,
    path = "/couple-space/memorial-days/default",
    tag = "情侣空间",
    params(("group_id" = i64, Query, description = "组ID")),
    responses(
        (status = 200, description = "获取成功", body = MemorialDay),
        (status = 404, description = "未找到", body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_default_memorial_day(
    state: State<Arc<AppState>>,
    query: Query<MemorialDayQuery>,
) -> Result<HttpResponse, CustomError> {
    let record = MemorialDayService::get_default_memorial_day(&state.db_pool, query.group_id).await?;
    Ok(HttpResponse::Ok().json(&record))
}
/// 创建纪念日
#[utoipa::path(
    post,
    path = "/couple-space/memorial-days",
    tag = "情侣空间",
    request_body = MemorialDayCreate,
    responses(
        (status = 200, description = "创建成功", body = MemorialDay),
        (status = 401, description = "未登录", body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn create_memorial_day(
    state: State<Arc<AppState>>,
    body: Json<MemorialDayCreate>,
) -> Result<HttpResponse, CustomError> {
    let record = MemorialDayService::create_memorial_day(&state.db_pool, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(&record))
}

/// 更新纪念日
#[utoipa::path(
    put,
    path = "/couple-space/memorial-days/{id}",
    tag = "情侣空间",
    params(
        ("id" = i64, Path, description = "纪念日ID"),
    ),
    request_body = MemorialDayUpdate,
    responses(
        (status = 200, description = "更新成功", body = MemorialDay),
        (status = 404, description = "未找到", body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_memorial_day(
    state: State<Arc<AppState>>,
    path: Path<i64>,
    body: Json<MemorialDayUpdate>,
) -> Result<HttpResponse, CustomError> {
    let record = MemorialDayService::update_memorial_day(
        &state.db_pool,
        path.into_inner(),
        body.group_id,
        body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(&record))
}

/// 删除纪念日
#[utoipa::path(
    delete,
    path = "/couple-space/memorial-days/{id}",
    tag = "情侣空间",
    params(
        ("id" = i64, Path, description = "纪念日ID"),
        ("group_id" = i64, Query, description = "组ID")
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 404, description = "未找到", body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn delete_memorial_day(
    state: State<Arc<AppState>>,
    path: Path<i64>,
    query: Query<MemorialDayQuery>,
) -> Result<HttpResponse, CustomError> {
    MemorialDayService::delete_memorial_day(&state.db_pool, path.into_inner(), query.group_id)
        .await?;
    Ok(HttpResponse::Ok().finish())
}
