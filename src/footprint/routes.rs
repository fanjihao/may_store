use crate::config::AppState;
use crate::errors::CustomError;
use crate::footprint::models::*;
use crate::footprint::service::FootprintService;
use crate::models::pagination::CursorPage;
use crate::users::models::user::UserToken;
use crate::users::service::UserService;
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
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let user = UserService::get_current_info(token.user_id, &state).await?;
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

/// 获取足迹记录详情
#[utoipa::path(
    get,
    path = "/footprint/records/{id}",
    params(
        ("id" = i64, Path, description = "足迹记录ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = RecordOut),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权查看"),
        (status = 404, description = "记录不存在")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn get_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let group_id = token
        .user
        .as_ref()
        .and_then(|u| u.group_id)
        .ok_or_else(|| CustomError::forbidden("请先加入组"))?;

    let res = FootprintService::get_record(
        &state.db_pool,
        token.user_id,
        group_id,
        id.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(&res))
}

/// 分页查询足迹记录列表 (统一时间轴，可选分组ID作为标签过滤)
#[utoipa::path(
    get,
    path = "/footprint/records",
    params(
        ("recordGroupId" = Option<i64>, Query, description = "足迹分组ID(标签过滤)"),
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
    query: Query<RecordQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = token
        .user
        .as_ref()
        .and_then(|u| u.group_id)
        .ok_or_else(|| CustomError::forbidden("请先加入组"))?;

    let q = query.into_inner();
    let res = FootprintService::list_records(
        &state.db_pool,
        token.user_id,
        group_id,
        q.record_group_id,
        q,
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

/// 修改足迹记录
#[utoipa::path(
    put,
    path = "/footprint/records/{id}",
    params(("id" = i64, Path, description = "足迹记录ID")),
    request_body = RecordUpdateInput,
    responses(
        (status = 200, description = "修改成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权修改")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn update_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    id: Path<i64>,
    input: Json<RecordUpdateInput>,
) -> Result<impl Responder, CustomError> {
    FootprintService::update_record(
        &state.db_pool,
        token.user_id,
        id.into_inner(),
        input.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({ "status": "ok" })))
}

/// 删除足迹记录
#[utoipa::path(
    delete,
    path = "/footprint/records/{id}",
    params(("id" = i64, Path, description = "足迹记录ID")),
    responses(
        (status = 200, description = "删除成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权删除")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn delete_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    FootprintService::delete_record(&state.db_pool, token.user_id, id.into_inner()).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({ "status": "ok" })))
}

/// 消耗钻石解锁足迹容量
#[utoipa::path(
    post,
    path = "/footprint/capacity/expand",
    responses(
        (status = 200, description = "解锁成功"),
        (status = 400, description = "钻石不足"),
        (status = 401, description = "未登录")
    ),
    tag = "足迹",
    security(("cookie_auth" = []))
)]
pub async fn expand_capacity(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let group_id = token
        .user
        .as_ref()
        .and_then(|u| u.group_id)
        .ok_or_else(|| CustomError::forbidden("请先加入组"))?;

    let new_capacity =
        FootprintService::expand_capacity(&state.db_pool, token.user_id, group_id).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({ "status": "ok", "newCapacity": new_capacity })))
}
