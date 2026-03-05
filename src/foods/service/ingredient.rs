use crate::errors::CustomError;
use crate::foods::models::ingredient::{
    BatchIngredientSortInput, IngredientCreateInput, IngredientOut, IngredientRecord, IngredientUpdateInput,
};
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use sqlx::PgPool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct IngredientCursor {
    pub sort: Option<i32>,
    pub name: String,
    pub ingredient_id: i64,
}

pub async fn list_ingredients(
    db: &PgPool,
    group_id: Option<i64>,
    keyword: &str,
    limit: i64,
    cursor_str: Option<&String>,
) -> Result<CursorPage<IngredientOut>, CustomError> {
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        r#"SELECT ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at
           FROM ingredients
           WHERE group_id = "#,
    );
    qb.push_bind(group_id);

    if !keyword.is_empty() {
        qb.push(" AND name ILIKE ");
        qb.push_bind(format!("%{}%", keyword));
    }

    if let Some(cursor_str) = cursor_str {
        if let Some(cursor) = decode_cursor::<IngredientCursor>(cursor_str) {
            qb.push(" AND (sort, name, ingredient_id) > (");
            qb.push_bind(cursor.sort);
            qb.push(", ");
            qb.push_bind(cursor.name);
            qb.push(", ");
            qb.push_bind(cursor.ingredient_id);
            qb.push(")");
        }
    }

    qb.push(" ORDER BY sort ASC, name ASC, ingredient_id ASC LIMIT ");
    qb.push_bind(limit + 1);

    let mut rows = qb.build_query_as::<IngredientRecord>().fetch_all(db).await?;

    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.pop();
    }

    let next_cursor = if has_more {
        rows.last().map(|r| {
            encode_cursor(&IngredientCursor {
                sort: r.sort,
                name: r.name.clone(),
                ingredient_id: r.ingredient_id,
            })
        })
    } else {
        None
    };

    let list: Vec<IngredientOut> = rows
        .into_iter()
        .map(|r| IngredientOut {
            ingredient_id: r.ingredient_id,
            name: r.name,
            group_id: r.group_id,
            unit: r.unit,
            calories: r.calories,
            description: r.description,
            icon: r.icon,
            sort: r.sort,
        })
        .collect();

    Ok(CursorPage {
        items: list,
        next_cursor,
        has_more,
        total: None,
    })
}

pub async fn get_ingredient(db: &PgPool, id: i64) -> Result<IngredientOut, CustomError> {
    let row = sqlx::query_as::<_, IngredientRecord>(
        "SELECT ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at FROM ingredients WHERE ingredient_id=$1"
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    match row {
        Some(r) => Ok(IngredientOut {
            ingredient_id: r.ingredient_id,
            name: r.name,
            group_id: r.group_id,
            unit: r.unit,
            calories: r.calories,
            description: r.description,
            icon: r.icon,
            sort: r.sort,
        }),
        None => Err(CustomError::NotFound("食材不存在".into())),
    }
}

pub async fn create_ingredient(
    db: &PgPool,
    data: &IngredientCreateInput,
    user_group_id: Option<i64>,
) -> Result<IngredientOut, CustomError> {
    let group_id = data.group_id.or(user_group_id);

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

    Ok(IngredientOut {
        ingredient_id: rec.ingredient_id,
        name: rec.name,
        group_id: rec.group_id,
        unit: rec.unit,
        calories: rec.calories,
        description: rec.description,
        icon: rec.icon,
        sort: rec.sort,
    })
}

pub async fn update_ingredient(
    db: &PgPool,
    id: i64,
    data: &IngredientUpdateInput,
) -> Result<IngredientOut, CustomError> {
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

    Ok(IngredientOut {
        ingredient_id: rec.ingredient_id,
        name: rec.name,
        group_id: rec.group_id,
        unit: rec.unit,
        calories: rec.calories,
        description: rec.description,
        icon: rec.icon,
        sort: rec.sort,
    })
}

pub async fn delete_ingredient(db: &PgPool, id: i64) -> Result<(), CustomError> {
    let deleted =
        sqlx::query("DELETE FROM ingredients WHERE ingredient_id=$1 RETURNING ingredient_id")
            .bind(id)
            .fetch_optional(db)
            .await?;

    if deleted.is_none() {
        return Err(CustomError::NotFound("食材不存在".into()));
    }
    Ok(())
}

pub async fn update_ingredients_sort(
    db: &PgPool,
    data: &BatchIngredientSortInput,
) -> Result<(), CustomError> {
    let items = &data.items;
    if items.is_empty() {
        return Ok(());
    }

    let mut tx = db.begin().await?;
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE ingredients SET sort = CASE ingredient_id ");
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

    Ok(())
}
