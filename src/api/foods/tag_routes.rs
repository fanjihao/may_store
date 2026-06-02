// API 层 - 标签路由（内部使用）
// 处理标签相关的 HTTP 请求

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use std::sync::Arc;

use crate::{
    config::AppState,
    errors::CustomError,
    domain::foods::food::FoodFilterQuery,
    domain::foods::tag::{
        BatchTagSortInput, FoodTagOut, TagCreateInput, TagUpdateInput,
    },
    application::food_service::TagService,
    middlewares::auth::UserToken,
};

/// 配置标签路由
#[allow(dead_code)]
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/food_tags")
            .route("", web::post().to(create_tag))
            .route("", web::get().to(get_tags))
            .route("/{id}", web::put().to(update_tag))
            .route("/{id}", web::delete().to(delete_tag)),
    )
    .service(web::scope("/food_tags").route("/sort", web::post().to(update_tags_sort)));
}

#[utoipa::path(
    post,
    path = "/food_tags",
    tag = "标签",
    request_body = TagCreateInput,
    responses((status = 201, body = FoodTagOut)),
    security(("cookie_auth" = []))
)]
pub async fn create_tag(
    token: UserToken,
    data: Json<TagCreateInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let out = TagService::create_tag(
        &state.db_pool,
        &data,
        token.user.as_ref().and_then(|u| u.group_id),
    )
    .await?;
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    get,
    path = "/food_tags",
    tag = "标签",
    params(FoodFilterQuery),
    responses((status = 200, body = Vec<FoodTagOut>)),
    security(("cookie_auth" = []))
)]
pub async fn get_tags(
    state: State<Arc<AppState>>,
    q: Query<FoodFilterQuery>,
) -> Result<impl Responder, CustomError> {
    let out = TagService::get_tags(&state.db_pool, &q).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    put,
    path = "/food_tags/{id}",
    tag = "标签",
    params(("id" = i64, Path, description = "标签ID")),
    request_body = TagUpdateInput,
    responses((status = 200, body = FoodTagOut), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn update_tag(
    _token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<TagUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let out = TagService::update_tag(&state.db_pool, *id, &data).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    post,
    path = "/food_tags/sort",
    tag = "标签",
    request_body = BatchTagSortInput,
    responses((status = 200, body = String)),
    security(("cookie_auth" = []))
)]
pub async fn update_tags_sort(
    _token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<BatchTagSortInput>,
) -> Result<impl Responder, CustomError> {
    TagService::update_tags_sort(&state.db_pool, &data).await?;
    Ok(HttpResponse::Ok().body("ok"))
}

#[utoipa::path(
    delete,
    path = "/food_tags/{id}",
    tag = "标签",
    params(("id" = i64, Path, description = "标签ID")),
    responses((status = 204), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn delete_tag(
    _token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    TagService::delete_tag(&state.db_pool, *id).await?;
    Ok(HttpResponse::NoContent())
}
