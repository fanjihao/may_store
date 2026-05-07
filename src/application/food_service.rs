// 应用服务层 - 菜品服务
// 包含菜品、标签、食材等业务用例

use sqlx::{PgPool, Row};
use crate::domain::foods::{food::*, ingredient::*, tag::*};
use crate::middlewares::auth::UserToken;
use crate::errors::CustomError;
use crate::models::pagination::CursorPage;

/// 菜品服务
pub struct FoodService;

impl FoodService {
    /// 创建菜品
    pub async fn create_food(
        db: &PgPool,
        token: &UserToken,
        input: &FoodCreateInput,
    ) -> Result<FoodOut, CustomError> {
        let rec = sqlx::query_as::<_, FoodRecord>(
            "INSERT INTO foods (group_id, name, description, images, price, status, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING food_id, group_id, name, description, images, tags, price, status, created_by, created_at, updated_at"
        )
        .bind(input.group_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.images)
        .bind(input.price)
        .bind(FoodStatus::Active)
        .bind(token.user_id as i64)
        .fetch_one(db)
        .await?;

        Ok(FoodOut::from_record(rec, false, false))
    }

    /// 获取菜品列表
    pub async fn get_foods(
        db: &PgPool,
        token: &UserToken,
        query: &FoodFilterQuery,
    ) -> Result<CursorPage<FoodOut>, CustomError> {
        let limit = query.limit.unwrap_or(50).clamp(1, 200);
        let group_id = query.group_id.or(token.user.as_ref().and_then(|u| u.group_id));

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT food_id, group_id, name, description, images, tags, price, status, created_by, created_at, updated_at FROM foods WHERE is_del = 0"
        );

        if let Some(gid) = group_id {
            qb.push(" AND group_id = ");
            qb.push_bind(gid);
        }

        if let Some(kw) = &query.keyword {
            qb.push(" AND name ILIKE '%' || ");
            qb.push_bind(kw);
            qb.push(" || '%'");
        }

        qb.push(" ORDER BY created_at DESC LIMIT ");
        qb.push_bind(limit + 1);

        let rows = qb.build().fetch_all(db).await?;

        let has_more = rows.len() > limit as usize;
        let items: Vec<FoodOut> = rows.into_iter()
            .take(limit as usize)
            .map(|r| {
                let tags: Vec<String> = r.get("tags");
                FoodOut {
                    food_id: r.get("food_id"),
                    group_id: r.get("group_id"),
                    name: r.get("name"),
                    description: r.get("description"),
                    images: r.get("images"),
                    tags,
                    price: r.get("price"),
                    status: r.get("status"),
                    created_by: r.get("created_by"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    is_liked: false,
                    is_done: false,
                }
            })
            .collect();

        Ok(CursorPage {
            items,
            next_cursor: None,
            has_more,
            total: None,
        })
    }

    /// 获取菜品详情
    pub async fn get_food_detail(
        db: &PgPool,
        token: Option<&UserToken>,
        food_id: i64,
    ) -> Result<FoodOut, CustomError> {
        let rec = sqlx::query_as::<_, FoodRecord>(
            "SELECT food_id, group_id, name, description, images, tags, price, status, created_by, created_at, updated_at FROM foods WHERE food_id = $1 AND is_del = 0"
        )
        .bind(food_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("菜品不存在".into()))?;

        // 检查用户是否标记过
        let (is_liked, is_done) = if let Some(t) = token {
            let marks: Vec<String> = sqlx::query(
                "SELECT mark_type::text FROM user_food_mark WHERE user_id=$1 AND food_id=$2"
            )
            .bind(t.user_id as i64)
            .bind(food_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|r| r.get(0))
            .collect();

            (
                marks.iter().any(|m| m == "Like"),
                marks.iter().any(|m| m == "Done"),
            )
        } else {
            (false, false)
        };

        Ok(FoodOut::from_record(rec, is_liked, is_done))
    }

    /// 更新菜品
    pub async fn update_food(
        db: &PgPool,
        token: &UserToken,
        food_id: i64,
        input: &FoodUpdateInput,
    ) -> Result<FoodOut, CustomError> {
        let rec = sqlx::query_as::<_, FoodRecord>(
            "UPDATE foods SET name = COALESCE($2, name), description = COALESCE($3, description), images = COALESCE($4, images), price = COALESCE($5, price) WHERE food_id = $1 RETURNING food_id, group_id, name, description, images, tags, price, status, created_by, created_at, updated_at"
        )
        .bind(food_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.images)
        .bind(input.price)
        .fetch_one(db)
        .await
        .map_err(|_| CustomError::NotFound("菜品不存在".into()))?;

        Ok(FoodOut::from_record(rec, false, false))
    }

    /// 删除菜品
    pub async fn delete_food(
        db: &PgPool,
        token: &UserToken,
        food_id: i64,
    ) -> Result<(), CustomError> {
        sqlx::query("UPDATE foods SET status = 'Deleted' WHERE food_id = $1")
            .bind(food_id)
            .execute(db)
            .await?;
        Ok(())
    }

    /// 标记菜品
    pub async fn mark_food(
        db: &PgPool,
        user_id: i64,
        food_id: i64,
        mark_type: MarkTypeEnum,
    ) -> Result<(), CustomError> {
        // 检查是否已标记
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM user_food_mark WHERE user_id=$1 AND food_id=$2 AND mark_type=$3"
        )
        .bind(user_id as i64)
        .bind(food_id)
        .bind(mark_type)
        .fetch_optional(db)
        .await?;

        if existing.is_some() {
            return Ok(());
        }

        sqlx::query(
            "INSERT INTO user_food_mark (user_id, food_id, mark_type) VALUES ($1, $2, $3)"
        )
        .bind(user_id as i64)
        .bind(food_id)
        .bind(mark_type)
        .execute(db)
        .await?;

        Ok(())
    }

    /// 取消标记
    pub async fn unmark_food(
        db: &PgPool,
        user_id: i64,
        food_id: i64,
        mark_type: MarkTypeEnum,
    ) -> Result<(), CustomError> {
        sqlx::query(
            "DELETE FROM user_food_mark WHERE user_id=$1 AND food_id=$2 AND mark_type=$3"
        )
        .bind(user_id as i64)
        .bind(food_id)
        .bind(mark_type)
        .execute(db)
        .await?;

        Ok(())
    }

    /// 获取已标记的菜品
    pub async fn get_marked_foods(
        db: &PgPool,
        token: &UserToken,
        query: &FoodFilterQuery,
    ) -> Result<CursorPage<FoodOut>, CustomError> {
        let limit = query.limit.unwrap_or(50).clamp(1, 200);

        let rows = sqlx::query_as::<_, FoodRecord>(
            r#"SELECT f.food_id, f.group_id, f.name, f.description, f.images, f.tags, f.price, f.status, f.created_by, f.created_at, f.updated_at
               FROM foods f
               JOIN user_food_mark m ON f.food_id = m.food_id
               WHERE m.user_id = $1 AND f.is_del = 0
               ORDER BY m.created_at DESC
               LIMIT $2"#
        )
        .bind(token.user_id as i64)
        .bind(limit + 1)
        .fetch_all(db)
        .await?;

        let has_more = rows.len() > limit as usize;
        let items: Vec<FoodOut> = rows.into_iter()
            .take(limit as usize)
            .map(|r| FoodOut::from_record(r, true, false))
            .collect();

        Ok(CursorPage {
            items,
            next_cursor: None,
            has_more,
            total: None,
        })
    }

    /// 盲盒抽取
    pub async fn draw_blind_box(
        db: &PgPool,
        token: &UserToken,
        input: &BlindBoxDrawInput,
    ) -> Result<BlindBoxDrawResultOut, CustomError> {
        let rec = sqlx::query_as::<_, FoodRecord>(
            "SELECT food_id, group_id, name, description, images, tags, price, status, created_by, created_at, updated_at FROM foods WHERE is_del = 0 AND status = 'Active' AND group_id = $1 ORDER BY RANDOM() LIMIT 1"
        )
        .bind(input.group_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("没有可抽取的菜品".into()))?;

        Ok(BlindBoxDrawResultOut {
            food: FoodOut::from_record(rec, false, false),
        })
    }
}

/// 食材服务
pub struct IngredientService;

impl IngredientService {
    pub async fn list_ingredients(
        db: &PgPool,
        group_id: Option<i64>,
        keyword: &str,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<CursorPage<IngredientOut>, CustomError> {
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, group_id, name, icon, sort, created_at, updated_at FROM ingredients WHERE 1=1"
        );

        if let Some(gid) = group_id {
            qb.push(" AND group_id = ");
            qb.push_bind(gid);
        }

        if !keyword.is_empty() {
            qb.push(" AND name ILIKE '%' || ");
            qb.push_bind(keyword);
            qb.push(" || '%'");
        }

        qb.push(" ORDER BY sort ASC, created_at DESC LIMIT ");
        qb.push_bind(limit + 1);

        let rows = qb.build().fetch_all(db).await?;

        let has_more = rows.len() > limit as usize;
        let items: Vec<IngredientOut> = rows.into_iter()
            .take(limit as usize)
            .map(|r| IngredientOut {
                id: r.get("id"),
                group_id: r.get("group_id"),
                name: r.get("name"),
                icon: r.get("icon"),
                sort: r.get("sort"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect();

        Ok(CursorPage {
            items,
            next_cursor: None,
            has_more,
            total: None,
        })
    }

    pub async fn get_ingredient(
        db: &PgPool,
        id: i64,
    ) -> Result<IngredientOut, CustomError> {
        let rec = sqlx::query_as::<_, IngredientRecord>(
            "SELECT id, group_id, name, icon, sort, created_at, updated_at FROM ingredients WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::NotFound("食材不存在".into()))?;

        Ok(IngredientOut {
            id: rec.id,
            group_id: rec.group_id,
            name: rec.name,
            icon: rec.icon,
            sort: rec.sort,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
        })
    }

    pub async fn create_ingredient(
        db: &PgPool,
        input: &IngredientCreateInput,
        group_id: Option<i64>,
    ) -> Result<IngredientOut, CustomError> {
        let rec = sqlx::query_as::<_, IngredientRecord>(
            "INSERT INTO ingredients (group_id, name, icon) VALUES ($1, $2, $3) RETURNING id, group_id, name, icon, sort, created_at, updated_at"
        )
        .bind(group_id)
        .bind(&input.name)
        .bind(&input.icon)
        .fetch_one(db)
        .await?;

        Ok(IngredientOut {
            id: rec.id,
            group_id: rec.group_id,
            name: rec.name,
            icon: rec.icon,
            sort: rec.sort,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
        })
    }

    pub async fn update_ingredient(
        db: &PgPool,
        id: i64,
        input: &IngredientUpdateInput,
    ) -> Result<IngredientOut, CustomError> {
        let rec = sqlx::query_as::<_, IngredientRecord>(
            "UPDATE ingredients SET name = COALESCE($2, name), icon = COALESCE($3, icon) WHERE id = $1 RETURNING id, group_id, name, icon, sort, created_at, updated_at"
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.icon)
        .fetch_one(db)
        .await
        .map_err(|_| CustomError::NotFound("食材不存在".into()))?;

        Ok(IngredientOut {
            id: rec.id,
            group_id: rec.group_id,
            name: rec.name,
            icon: rec.icon,
            sort: rec.sort,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
        })
    }

    pub async fn delete_ingredient(
        db: &PgPool,
        id: i64,
    ) -> Result<(), CustomError> {
        sqlx::query("DELETE FROM ingredients WHERE id = $1")
            .bind(id)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn update_ingredients_sort(
        db: &PgPool,
        input: &BatchIngredientSortInput,
    ) -> Result<(), CustomError> {
        let mut tx = db.begin().await?;

        for item in &input.items {
            sqlx::query("UPDATE ingredients SET sort = $2 WHERE id = $1")
                .bind(item.ingredient_id)
                .bind(item.sort)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

/// 标签服务
pub struct TagService;

impl TagService {
    pub async fn create_tag(
        db: &PgPool,
        input: &TagCreateInput,
        group_id: Option<i64>,
    ) -> Result<FoodTagOut, CustomError> {
        let rec = sqlx::query_as::<_, TagRecord>(
            "INSERT INTO tags (group_id, name, color) VALUES ($1, $2, $3) RETURNING id, group_id, name, color, sort, created_at, updated_at"
        )
        .bind(group_id)
        .bind(&input.name)
        .bind(&input.color)
        .fetch_one(db)
        .await?;

        Ok(FoodTagOut {
            id: rec.id,
            group_id: rec.group_id,
            name: rec.name,
            color: rec.color,
            sort: rec.sort,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
        })
    }

    pub async fn get_tags(
        db: &PgPool,
        query: &FoodFilterQuery,
    ) -> Result<Vec<FoodTagOut>, CustomError> {
        let group_id = query.group_id;

        let rows = sqlx::query_as::<_, TagRecord>(
            "SELECT id, group_id, name, color, sort, created_at, updated_at FROM tags WHERE is_del = 0 ORDER BY sort ASC"
        )
        .fetch_all(db)
        .await?;

        let items: Vec<FoodTagOut> = rows.into_iter()
            .map(|r| FoodTagOut {
                id: r.id,
                group_id: r.group_id,
                name: r.name,
                color: r.color,
                sort: r.sort,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect();

        Ok(items)
    }

    pub async fn update_tag(
        db: &PgPool,
        id: i64,
        input: &TagUpdateInput,
    ) -> Result<FoodTagOut, CustomError> {
        let rec = sqlx::query_as::<_, TagRecord>(
            "UPDATE tags SET name = COALESCE($2, name), color = COALESCE($3, color) WHERE id = $1 RETURNING id, group_id, name, color, sort, created_at, updated_at"
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.color)
        .fetch_one(db)
        .await
        .map_err(|_| CustomError::NotFound("标签不存在".into()))?;

        Ok(FoodTagOut {
            id: rec.id,
            group_id: rec.group_id,
            name: rec.name,
            color: rec.color,
            sort: rec.sort,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
        })
    }

    pub async fn delete_tag(
        db: &PgPool,
        id: i64,
    ) -> Result<(), CustomError> {
        sqlx::query("UPDATE tags SET is_del = 1 WHERE id = $1")
            .bind(id)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn update_tags_sort(
        db: &PgPool,
        input: &BatchTagSortInput,
    ) -> Result<(), CustomError> {
        let mut tx = db.begin().await?;

        for item in &input.sorts {
            sqlx::query("UPDATE tags SET sort = $2 WHERE id = $1")
                .bind(item.id)
                .bind(item.sort)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}