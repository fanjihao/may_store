use crate::config::AppState;
use crate::errors::CustomError;
use crate::footprint::models::*;
use crate::footprint::service::FootprintService;
use crate::models::pagination::CursorPage;
use crate::users::models::user::UserToken;
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

/// 获取足迹概览数据
#[utoipa::path(
    get,
    path = "/footprint/overview",
    responses(
        (status = 200, description = "获取成功", body = FootprintOverview),
        (status = 401, description = "未登录"),
        (status = 403, description = "未加入组")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn get_overview(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let group_id = token
        .user
        .as_ref()
        .and_then(|u| u.group_id)
        .ok_or_else(|| CustomError::forbidden("请先加入组"))?;

    let res = FootprintService::get_overview(&state.db_pool, token.user_id, group_id).await?;
    Ok(HttpResponse::Ok().json(&res))
}

/// 校验用户是否有权限进入足迹模块
#[utoipa::path(
    get,
    path = "/footprint/check-permission",
    responses(
        (status = 200, description = "获取成功", body = CheckPermissionResponse),
        (status = 401, description = "未登录")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn check_permission(
    _state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let user = token
        .user
        .as_ref()
        .ok_or_else(|| CustomError::unauthorized("未登录"))?;
    Ok(HttpResponse::Ok().json(&CheckPermissionResponse {
        has_group: user.group_id.is_some(),
        group_id: user.group_id,
    }))
}

/// 获取足迹故事分组列表
#[utoipa::path(
    get,
    path = "/footprint/groups",
    responses(
        (status = 200, description = "获取成功", body = Vec<RecordGroup>),
        (status = 401, description = "未登录"),
        (status = 403, description = "未加入组")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn get_groups(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let group_id = token
        .user
        .as_ref()
        .and_then(|u| u.group_id)
        .ok_or_else(|| CustomError::forbidden("请先加入组"))?;

    let res = FootprintService::list_record_groups(&state.db_pool, group_id).await?;
    Ok(HttpResponse::Ok().json(&res))
}

/// 分页查询足迹记录列表
#[utoipa::path(
    get,
    path = "/footprint/groups/{id}/records",
    params(
        ("id" = i64, Path, description = "足迹分组ID"),
        RecordQuery
    ),
    responses(
        (status = 200, description = "获取成功", body = CursorPage<RecordOut>),
        (status = 401, description = "未登录"),
        (status = 403, description = "未加入组")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn get_records(
    state: State<Arc<AppState>>,
    token: UserToken,
    id: Path<i64>,
    query: Query<RecordQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = token
        .user
        .as_ref()
        .and_then(|u| u.group_id)
        .ok_or_else(|| CustomError::forbidden("请先加入组"))?;

    let rg_id = id.into_inner();

    let res = FootprintService::list_records(
        &state.db_pool,
        token.user_id,
        group_id,
        rg_id,
        query.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(&res))
}

/// 提交新的足迹记录
#[utoipa::path(
    post,
    path = "/footprint/records",
    request_body = RecordCreateInput,
    responses(
        (status = 200, description = "提交成功", body = SubmitRecordResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "未加入组")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn submit_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    input: Json<RecordCreateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = token
        .user
        .as_ref()
        .and_then(|u| u.group_id)
        .ok_or_else(|| CustomError::forbidden("请先加入组"))?;

    let id = FootprintService::submit_record(
        &state.db_pool,
        token.user_id,
        group_id,
        input.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(&SubmitRecordResponse { record_id: id }))
}
