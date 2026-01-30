use crate::{
    errors::CustomError,
    models::foods::{IngredientCreateInput, IngredientOut, IngredientRecord, IngredientUpdateInput},
    AppState,
};
use ntex::web::{
    types::{Json, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

// ================= Ingredient CRUD =================

#[derive(Debug, serde::Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientQuery {
    #[serde(rename = "groupId")]
    pub group_id: Option<i64>,
    pub keyword: Option<String>,
    #[serde(default)]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[utoipa::path(
    get,
    path = "/ingredients",
    tag = "食材",
    params(
        ("groupId" = Option<i64>, Query, description = "组ID筛选"),
        ("keyword" = Option<String>, Query, description = "搜索食材名称"),
        ("limit" = i64, Query, description = "返回条数，默认50"),
        ("offset" = i64, Query, description = "偏移量，默认0"),
    ),
    responses((status = 200, body = [IngredientOut])),
    security(("cookie_auth" = []))
)]
pub async fn list_ingredients(
    user_token: crate::models::users::UserToken,
    state: State<Arc<AppState>>,
    query: Query<IngredientQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 使用用户所属组或请求中的组ID
    let group_id = query.group_id.or(user_token.user.as_ref().and_then(|u| u.group_id));
    let keyword = query.keyword.as_deref().unwrap_or("");
    let limit = query.limit.clamp(1, 200);
    let offset = query.offset;

    let sql = r#"
        SELECT ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at
        FROM ingredients
        WHERE group_id = $1
        ORDER BY sort ASC, name ASC
        LIMIT $3 OFFSET $4
    "#;

    let rows = sqlx::query_as::<_, IngredientRecord>(sql)
        .bind(group_id)
        .bind(format!("%{}%", keyword))
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?;

    let list: Vec<IngredientOut> = rows.into_iter().map(|r| IngredientOut {
        ingredient_id: r.ingredient_id,
        name: r.name,
        group_id: r.group_id,
        unit: r.unit,
        calories: r.calories,
        description: r.description,
        icon: r.icon,
        sort: r.sort,
    }).collect();

    Ok(HttpResponse::Ok().json(&list))
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
    _user_token: crate::models::users::UserToken,
    state: State<Arc<AppState>>,
    id: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let id = *id;

    let row = sqlx::query_as::<_, IngredientRecord>(
        "SELECT ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at FROM ingredients WHERE ingredient_id=$1"
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    match row {
        Some(r) => {
            let out = IngredientOut {
                ingredient_id: r.ingredient_id,
                name: r.name,
                group_id: r.group_id,
                unit: r.unit,
                calories: r.calories,
                description: r.description,
                icon: r.icon,
                sort: r.sort,
            };
            Ok(HttpResponse::Ok().json(&out))
        }
        None => Err(CustomError::NotFound("食材不存在".into())),
    }
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
    user_token: crate::models::users::UserToken,
    state: State<Arc<AppState>>,
    data: Json<IngredientCreateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    // 使用用户的组ID或请求中的组ID
    let group_id = data.group_id.or(user_token.user.as_ref().and_then(|u| u.group_id));

    let rec = sqlx::query_as::<_, IngredientRecord>(
        r#"INSERT INTO ingredients (name, group_id, unit, calories, description, icon, sort)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at"#
    )
    .bind(&data.name)
    .bind(group_id)
    .bind(data.unit.as_ref())
    .bind(data.calories)
    .bind(data.description.as_ref())
    .bind(data.icon.as_ref())
    .bind(data.sort.unwrap_or(0))
    .fetch_one(db)
    .await?;

    let out = IngredientOut {
        ingredient_id: rec.ingredient_id,
        name: rec.name,
        group_id: rec.group_id,
        unit: rec.unit,
        calories: rec.calories,
        description: rec.description,
        icon: rec.icon,
        sort: rec.sort,
    };

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
    _user_token: crate::models::users::UserToken,
    state: State<Arc<AppState>>,
    id: ntex::web::types::Path<i64>,
    data: Json<IngredientUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let id = *id;

    // Check exists
    let exists = sqlx::query("SELECT 1 FROM ingredients WHERE ingredient_id=$1")
        .bind(id)
        .fetch_optional(db)
        .await?;

    if exists.is_none() {
        return Err(CustomError::NotFound("食材不存在".into()));
    }

    let rec = sqlx::query_as::<_, IngredientRecord>(
        r#"UPDATE ingredients
           SET name = COALESCE($2, name),
               unit = COALESCE($3, unit),
               calories = COALESCE($4, calories),
               description = COALESCE($5, description),
               icon = COALESCE($6, icon),
               sort = COALESCE($7, sort),
               updated_at = NOW()
           WHERE ingredient_id = $1
           RETURNING ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at"#
    )
    .bind(id)
    .bind(data.name.as_ref())
    .bind(data.unit.as_ref())
    .bind(data.calories)
    .bind(data.description.as_ref())
    .bind(data.icon.as_ref())
    .bind(data.sort)
    .fetch_one(db)
    .await?;

    let out = IngredientOut {
        ingredient_id: rec.ingredient_id,
        name: rec.name,
        group_id: rec.group_id,
        unit: rec.unit,
        calories: rec.calories,
        description: rec.description,
        icon: rec.icon,
        sort: rec.sort,
    };

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
    _user_token: crate::models::users::UserToken,
    state: State<Arc<AppState>>,
    id: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let id = *id;

    let deleted = sqlx::query("DELETE FROM ingredients WHERE ingredient_id=$1 RETURNING ingredient_id")
        .bind(id)
        .fetch_optional(db)
        .await?;

    if deleted.is_none() {
        return Err(CustomError::NotFound("食材不存在".into()));
    }

    Ok(HttpResponse::NoContent())
}

use crate::models::foods::BatchIngredientSortInput;

#[utoipa::path(
    post,
    path = "/ingredients/sort",
    tag = "食材",
    request_body = BatchIngredientSortInput,
    responses((status = 200, body = String)),
    security(("cookie_auth" = []))
)]
pub async fn update_ingredients_sort(
    _user_token: crate::models::users::UserToken,
    state: State<Arc<AppState>>,
    data: Json<BatchIngredientSortInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let items = &data.items;

    if items.is_empty() {
        return Ok(HttpResponse::Ok().body("ok"));
    }

    let mut tx = db.begin().await?;

    // 使用 QueryBuilder 动态构建批量更新语句
    // UPDATE ingredients SET sort = CASE ingredient_id WHEN 1 THEN 10 WHEN 2 THEN 20 ELSE sort END WHERE ingredient_id IN (1, 2)
    use sqlx::QueryBuilder;

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("UPDATE ingredients SET sort = CASE ingredient_id ");
    let mut ids = Vec::new();

    for item in items {
        qb.push("WHEN ");
        qb.push_bind(item.ingredient_id);
        qb.push(" THEN ");
        qb.push_bind(item.sort);
        qb.push(" ");
        ids.push(item.ingredient_id);
    }

    qb.push("ELSE sort END WHERE ingredient_id IN (");

    let mut separated = qb.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    qb.build().execute(&mut *tx).await?;

    tx.commit().await?;

    Ok(HttpResponse::Ok().body("ok"))
}
