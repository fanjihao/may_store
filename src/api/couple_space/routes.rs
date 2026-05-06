// API 层 - 情侣空间路由
// 处理情侣空间纪念日相关的 HTTP 请求

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use std::sync::Arc;

use crate::{
    application::couple_space_service::MemorialDayService,
    config::AppState,
    domain::couple_space::{
        MemorialDay, MemorialDayCreate, MemorialDayCursor, MemorialDayQuery, MemorialDayUpdate,
    },
    errors::CustomError,
    models::pagination::{decode_cursor, encode_cursor, CursorPage},
};

/// 配置情侣空间路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/couple-space/memorial-days")
            .route("", web::get().to(list_memorial_days))
            .route("", web::post().to(create_memorial_day))
            .route("/{id}", web::put().to(update_memorial_day))
            .route("/{id}", web::delete().to(delete_memorial_day))
            .route("/default", web::get().to(get_default_memorial_day)),
    );
}

#[utoipa::path(
    get,
    path = "/couple-space/memorial-days",
    tag = "情侣空间",
    params(MemorialDayQuery),
    responses(
        (status = 200, description = "获取成功", body = Vec<MemorialDay>)
    )
)]
pub async fn list_memorial_days(
    _token: crate::middlewares::auth::UserToken,
    state: State<Arc<AppState>>,
    query: Query<MemorialDayQuery>,
) -> Result<impl Responder, CustomError> {
    Ok(HttpResponse::Ok().json(&Vec::<MemorialDay>::new()))
}

#[utoipa::path(
    post,
    path = "/couple-space/memorial-days",
    tag = "情侣空间",
    request_body = MemorialDayCreate,
    responses(
        (status = 201, description = "创建成功", body = MemorialDay)
    )
)]
pub async fn create_memorial_day(
    _token: crate::middlewares::auth::UserToken,
    _state: State<Arc<AppState>>,
    _data: Json<MemorialDayCreate>,
) -> Result<impl Responder, CustomError> {
    Ok(HttpResponse::Ok().json(&MemorialDay {
        id: 0,
        user_id: 0,
        couple_user_id: 0,
        name: "".to_string(),
        date: chrono::Utc::now().date_naive(),
        day_type: "".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }))
}

#[utoipa::path(
    put,
    path = "/couple-space/memorial-days/{id}",
    tag = "情侣空间",
    params(("id" = i64, Path, description = "纪念日ID")),
    request_body = MemorialDayUpdate,
    responses(
        (status = 200, description = "更新成功")
    )
)]
pub async fn update_memorial_day(
    _token: crate::middlewares::auth::UserToken,
    _state: State<Arc<AppState>>,
    _id: Path<i64>,
    _data: Json<MemorialDayUpdate>,
) -> Result<impl Responder, CustomError> {
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    delete,
    path = "/couple-space/memorial-days/{id}",
    tag = "情侣空间",
    params(("id" = i64, Path, description = "纪念日ID")),
    responses(
        (status = 200, description = "删除成功")
    )
)]
pub async fn delete_memorial_day(
    _token: crate::middlewares::auth::UserToken,
    _state: State<Arc<AppState>>,
    _id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    get,
    path = "/couple-space/memorial-days/default",
    tag = "情侣空间",
    responses(
        (status = 200, description = "获取成功", body = MemorialDay)
    )
)]
pub async fn get_default_memorial_day(
    _token: crate::middlewares::auth::UserToken,
    _state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    Ok(HttpResponse::Ok().json(&serde_json::json!(null)))
}
