use crate::errors::CustomError;
use crate::foods::models::food::FoodFilterQuery;
use crate::foods::models::tag::{
    BatchTagSortInput, FoodTagOut, TagCreateInput, TagRecord, TagUpdateInput,
};
use sqlx::PgPool;
use std::collections::HashMap;

pub struct TagService;
impl TagService {
    pub async fn create_tag(
        db: &PgPool,
        data: &TagCreateInput,
        user_group_id: Option<i64>,
    ) -> Result<FoodTagOut, CustomError> {
        let gid = data.group_id.or(user_group_id);
        let rec = sqlx::query_as::<_, TagRecord>(
            "INSERT INTO tags (tag_name, icon, group_id, sort) VALUES ($1,$2,$3,$4) RETURNING tag_id, tag_name, icon, group_id, sort, created_at"
        )
        .bind(&data.tag_name)
        .bind(data.icon.as_ref())
        .bind(gid)
        .bind(data.sort)
        .fetch_one(db)
        .await?;

        Ok(FoodTagOut {
            tag_id: rec.tag_id,
            tag_name: rec.tag_name,
            icon: rec.icon,
            sort: rec.sort,
            food_count: None,
        })
    }

    pub async fn get_tags(
        db: &PgPool,
        query: &FoodFilterQuery,
    ) -> Result<Vec<FoodTagOut>, CustomError> {
        let group_id = match query.group_id {
            Some(id) => id,
            None => return Err(CustomError::BadRequest("缺少group_id参数".into())),
        };

        let rows: Vec<TagRecord> = sqlx::query_as(
            "SELECT tag_id, tag_name, icon, group_id, sort, created_at FROM tags WHERE group_id=$1 ORDER BY sort NULLS LAST, tag_id"
        )
        .bind(group_id)
        .fetch_all(db)
        .await?;

        let tag_ids: Vec<i64> = rows.iter().map(|r| r.tag_id).collect();
        let mut counts_map: HashMap<i64, i64> = HashMap::new();

        if !tag_ids.is_empty() {
            use sqlx::Row;
            let count_rows = sqlx::query(
                "SELECT tag_id, COUNT(*) as cnt FROM foods WHERE tag_id = ANY($1) AND is_del=0 GROUP BY tag_id"
            )
            .bind(&tag_ids)
            .fetch_all(db)
            .await?;

            for r in count_rows {
                let tid: i64 = r.get("tag_id");
                let cnt: i64 = r.get("cnt");
                counts_map.insert(tid, cnt);
            }
        }

        Ok(rows
            .into_iter()
            .map(|r| {
                let cnt = counts_map.get(&r.tag_id).copied().unwrap_or(0);
                FoodTagOut {
                    tag_id: r.tag_id,
                    tag_name: r.tag_name,
                    icon: r.icon,
                    sort: r.sort,
                    food_count: Some(cnt),
                }
            })
            .collect())
    }

    pub async fn update_tag(
        db: &PgPool,
        id: i64,
        data: &TagUpdateInput,
    ) -> Result<FoodTagOut, CustomError> {
        let exists = sqlx::query("SELECT 1 FROM tags WHERE tag_id=$1")
            .bind(id)
            .fetch_optional(db)
            .await?;
        if exists.is_none() {
            return Err(CustomError::NotFound("标签不存在".into()));
        }

        let rec = sqlx::query_as::<_, TagRecord>(
            "UPDATE tags SET tag_name = COALESCE($2, tag_name), icon = COALESCE($3, icon), sort = COALESCE($4, sort) WHERE tag_id=$1 RETURNING tag_id, tag_name, icon, group_id, sort, created_at"
        )
        .bind(id)
        .bind(data.tag_name.as_ref())
        .bind(data.icon.as_ref())
        .bind(data.sort)
        .fetch_one(db)
        .await?;

        Ok(FoodTagOut {
            tag_id: rec.tag_id,
            tag_name: rec.tag_name,
            icon: rec.icon,
            sort: rec.sort,
            food_count: None,
        })
    }

    pub async fn delete_tag(db: &PgPool, id: i64) -> Result<(), CustomError> {
        let mut tx = db.begin().await?;
        // check usage
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM foods WHERE tag_id=$1 AND is_del=0")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if count.0 > 0 {
            return Err(CustomError::BadRequest("标签正在使用中，无法删除".into()));
        }
        sqlx::query("DELETE FROM tags WHERE tag_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn update_tags_sort(
        db: &PgPool,
        data: &BatchTagSortInput,
    ) -> Result<(), CustomError> {
        if data.items.is_empty() {
            return Ok(());
        }

        let mut tx = db.begin().await?;
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("UPDATE tags SET sort = CASE tag_id ");
        let mut ids = Vec::new();

        for item in &data.items {
            qb.push("WHEN ");
            qb.push_bind(item.tag_id);
            qb.push(" THEN ");
            qb.push_bind(item.sort);
            qb.push(" ");
            ids.push(item.tag_id);
        }

        qb.push("ELSE sort END WHERE tag_id IN (");
        let mut separated = qb.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        qb.build().execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
