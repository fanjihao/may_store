use crate::foods::service::ingredient::IngredientService;
use crate::{
    config::AppState,
    errors::CustomError,
    foods::models::ingredient::{
        BatchIngredientSortInput, IngredientCreateInput, IngredientOut, IngredientUpdateInput,
    },
    models::pagination::CursorPage, users::models::user::UserToken,
};
use ntex::web::{
    types::{Json, Path, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

#[derive(Debug, serde::Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct IngredientQuery {
    pub group_id: Option<i64>,
    pub keyword: Option<String>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/ingredients",
    tag = "食材",
    params(IngredientQuery),
    responses((status = 200, body = CursorPage<IngredientOut>)),
    security(("cookie_auth" = []))
)]
pub async fn list_ingredients(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<IngredientQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = query
        .group_id
        .or(user_token.user.as_ref().and_then(|u| u.group_id));
    let keyword = query.keyword.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let out = IngredientService::list_ingredients(
        &state.db_pool,
        group_id,
        keyword,
        limit,
        query.cursor.as_ref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    get,
    path = "/ingredients/{id}",
    tag = "食材",
    params(("id" = i64, Path, description = "食材ID")),
    responses((status = 200, body = IngredientOut), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn get_ingredient(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let out = IngredientService::get_ingredient(&state.db_pool, *id).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    post,
    path = "/ingredients",
    tag = "食材",
    request_body = IngredientCreateInput,
    responses((status = 201, body = IngredientOut)),
    security(("cookie_auth" = []))
)]
pub async fn create_ingredient(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<IngredientCreateInput>,
) -> Result<impl Responder, CustomError> {
    let out = IngredientService::create_ingredient(
        &state.db_pool,
        &data,
        user_token.user.as_ref().and_then(|u| u.group_id),
    )
    .await?;
    Ok(HttpResponse::Created().json(&out))
}

#[utoipa::path(
    put,
    path = "/ingredients/{id}",
    tag = "食材",
    params(("id" = i64, Path, description = "食材ID")),
    request_body = IngredientUpdateInput,
    responses((status = 200, body = IngredientOut), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn update_ingredient(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<IngredientUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let out = IngredientService::update_ingredient(&state.db_pool, *id, &data).await?;
    Ok(HttpResponse::Ok().json(&out))
}

#[utoipa::path(
    delete,
    path = "/ingredients/{id}",
    tag = "食材",
    params(("id" = i64, Path, description = "食材ID")),
    responses((status = 204), (status = 404, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn delete_ingredient(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    IngredientService::delete_ingredient(&state.db_pool, *id).await?;
    Ok(HttpResponse::NoContent())
}

#[utoipa::path(
    post,
    path = "/ingredients/sort",
    tag = "食材",
    request_body = BatchIngredientSortInput,
    responses((status = 200, body = String)),
    security(("cookie_auth" = []))
)]
pub async fn update_ingredients_sort(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<BatchIngredientSortInput>,
) -> Result<impl Responder, CustomError> {
    IngredientService::update_ingredients_sort(&state.db_pool, &data).await?;
    Ok(HttpResponse::Ok().body("ok"))
}
