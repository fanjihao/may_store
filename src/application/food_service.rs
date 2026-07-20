// 应用服务层 - 食材排序
//
// 菜品和标签 API 已由 src/api/foods 与 src/api/tags 直接实现。
// 这里只保留仍被 ingredients 路由调用的服务，避免维护旧版 foods.tags/tags.id SQL。

use crate::domain::foods::ingredient::BatchIngredientSortInput;
use crate::errors::CustomError;
use sqlx::PgPool;

/// 食材服务
pub struct IngredientService;

impl IngredientService {
    pub async fn update_ingredients_sort(
        db: &PgPool,
        input: &BatchIngredientSortInput,
    ) -> Result<(), CustomError> {
        let mut tx = db.begin().await?;

        for item in &input.items {
            sqlx::query("UPDATE ingredients SET sort = $2 WHERE ingredient_id = $1")
                .bind(item.ingredient_id)
                .bind(item.sort)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
