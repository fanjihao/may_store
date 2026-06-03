// API - 足迹路由
// FSD.latest.md compliant - 组内足迹、纪念内容、图片

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::footprint_service::FootprintService;
use crate::config::AppState;
use crate::domain::footprint::{RecordCreateInput, RecordGroup, RecordOut, RecordQuery, RecordUpdateInput};
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::models::pagination::CursorPage;

/// 配置足迹路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/footprints")
            .route("/groups", web::get().to(list_record_groups))
            .route("/records", web::post().to(create_record))
            .route("/records/submit", web::post().to(submit_record))
            .route("/records/{record_id}", web::get().to(get_record))
            .route("/records/{record_id}", web::put().to(update_record))
            .route("/records/{record_id}", web::delete().to(delete_record))
            .route("/records/list", web::get().to(list_records))
            .route("/overview", web::get().to(get_overview)),
    );
}

// ========== 响应结构 ==========

/// 足迹概览响应
#[derive(Debug, Serialize, ToSchema)]
pub struct FootprintOverviewResponse {
    pub together_days: i32,
    pub total_feedings: i32,
    pub streak_days: i32,
    pub total_records: i32,
    pub streak_progress: f32,
    pub feeding_text: String,
    pub diamond_balance: i32,
    pub footprint_capacity: i32,
    pub footprint_count: i32,
}

/// 足迹分组列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct RecordGroupsResponse {
    pub items: Vec<RecordGroup>,
}

// ========== 处理器 ==========

/// 获取足迹概览
/// GET /api/footprints/overview?group_id=xxx
#[utoipa::path(
    get,
    path = "/api/footprints/overview",
    tag = "足迹",
    params(
        ("group_id" = i64, Query, description = "小组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = FootprintOverviewResponse),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_overview(
    state: State<Arc<AppState>>,
    token: UserToken,
    query: Query<FootprintOverviewQuery>,
) -> Result<impl Responder, CustomError> {
    let overview = FootprintService::get_overview(&state.db_pool, token.user_id, query.group_id).await?;
    Ok(HttpResponse::Ok().json(&FootprintOverviewResponse {
        together_days: overview.together_days,
        total_feedings: overview.total_feedings,
        streak_days: overview.streak_days,
        total_records: overview.total_records,
        streak_progress: overview.streak_progress,
        feeding_text: overview.feeding_text,
        diamond_balance: overview.diamond_balance,
        footprint_capacity: overview.footprint_capacity,
        footprint_count: overview.footprint_count,
    }))
}

/// 足迹概览查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct FootprintOverviewQuery {
    pub group_id: i64,
}

/// 获取足迹分组列表
/// GET /api/footprints/groups?group_id=xxx
#[utoipa::path(
    get,
    path = "/api/footprints/groups",
    tag = "足迹",
    params(
        ("group_id" = i64, Query, description = "小组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = RecordGroupsResponse),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn list_record_groups(
    state: State<Arc<AppState>>,
    query: Query<FootprintOverviewQuery>,
) -> Result<impl Responder, CustomError> {
    let groups = FootprintService::list_record_groups(&state.db_pool, query.group_id).await?;
    Ok(HttpResponse::Ok().json(&RecordGroupsResponse { items: groups }))
}

/// 创建足迹记录（草稿）
/// POST /api/footprints/records
#[utoipa::path(
    post,
    path = "/api/footprints/records",
    tag = "足迹",
    request_body = RecordCreateInput,
    responses(
        (status = 201, description = "创建成功", body = RecordOut),
        (status = 400, description = "参数错误"),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn create_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    input: Json<RecordCreateInput>,
) -> Result<impl Responder, CustomError> {
    let record_id = FootprintService::create_record(&state.db_pool, token.user_id, &input).await?;
    let record = FootprintService::get_record(&state.db_pool, token.user_id, input.record_group_id, record_id).await?;
    Ok(HttpResponse::Ok().json(&record))
}

/// 提交足迹记录（正式发布）
/// POST /api/footprints/records/submit
#[utoipa::path(
    post,
    path = "/api/footprints/records/submit",
    tag = "足迹",
    request_body = RecordCreateInput,
    responses(
        (status = 201, description = "提交成功", body = RecordOut),
        (status = 400, description = "容量已满或其他错误"),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn submit_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    input: Json<RecordCreateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = input.record_group_id;
    let record_id = FootprintService::submit_record(&state.db_pool, token.user_id, group_id, input.into_inner()).await?;
    let record = FootprintService::get_record(&state.db_pool, token.user_id, group_id, record_id).await?;
    Ok(HttpResponse::Ok().json(&record))
}

/// 获取足迹记录详情
/// GET /api/footprints/records/{record_id}?group_id=xxx
#[utoipa::path(
    get,
    path = "/api/footprints/records/{record_id}",
    tag = "足迹",
    params(
        ("record_id" = i64, Path, description = "记录ID"),
        ("group_id" = i64, Query, description = "小组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = RecordOut),
        (status = 401, description = "未登录"),
        (status = 404, description = "记录不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    query: Query<FootprintOverviewQuery>,
) -> Result<impl Responder, CustomError> {
    let record_id = path.into_inner();
    let record = FootprintService::get_record(&state.db_pool, token.user_id, query.group_id, record_id).await?;
    Ok(HttpResponse::Ok().json(&record))
}

/// 更新足迹记录
/// PUT /api/footprints/records/{record_id}
#[utoipa::path(
    put,
    path = "/api/footprints/records/{record_id}",
    tag = "足迹",
    params(
        ("record_id" = i64, Path, description = "记录ID")
    ),
    request_body = RecordUpdateInput,
    responses(
        (status = 200, description = "更新成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权修改"),
        (status = 404, description = "记录不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    input: Json<RecordUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let record_id = path.into_inner();
    FootprintService::update_record(&state.db_pool, token.user_id, record_id, input.into_inner()).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({"status": "ok"})))
}

/// 删除足迹记录
/// DELETE /api/footprints/records/{record_id}?group_id=xxx
#[utoipa::path(
    delete,
    path = "/api/footprints/records/{record_id}",
    tag = "足迹",
    params(
        ("record_id" = i64, Path, description = "记录ID"),
        ("group_id" = i64, Query, description = "小组ID")
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权删除"),
        (status = 404, description = "记录不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn delete_record(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    _query: Query<FootprintOverviewQuery>,
) -> Result<impl Responder, CustomError> {
    let record_id = path.into_inner();
    FootprintService::delete_record(&state.db_pool, token.user_id, record_id).await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({"status": "ok"})))
}

/// 获取足迹记录列表
/// GET /api/footprints/records/list
#[utoipa::path(
    get,
    path = "/api/footprints/records/list",
    tag = "足迹",
    params(
        ("group_id" = i64, Query, description = "小组ID"),
        ("record_group_id" = Option<i64>, Query, description = "分组ID"),
        ("limit" = Option<i64>, Query, description = "每页数量"),
        ("cursor" = Option<String>, Query, description = "游标")
    ),
    responses(
        (status = 200, description = "获取成功", body = CursorPage<RecordOut>),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn list_records(
    state: State<Arc<AppState>>,
    token: UserToken,
    query: Query<RecordQuery>,
) -> Result<impl Responder, CustomError> {
    let records = FootprintService::list_records(
        &state.db_pool,
        token.user_id,
        query.group_id,
        query.record_group_id,
        query.clone(),
    ).await?;
    Ok(HttpResponse::Ok().json(&records))
}