use super::models::{MemorialDay, MemorialDayCreate, MemorialDayUpdate};
use crate::errors::CustomError;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};

pub struct MemorialDayService;

impl MemorialDayService {
    pub async fn list_memorial_days(
        pool: &PgPool,
        group_id: i64,
        limit: i64,
        cursor_condition: Option<(chrono::NaiveDate, i64)>,
    ) -> Result<(Vec<MemorialDay>, i64), CustomError> {
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memorial_day WHERE group_id = $1")
                .bind(group_id)
                .fetch_one(pool)
                .await?;

        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM memorial_day WHERE group_id = ");
        qb.push_bind(group_id);

        if let Some((date, id)) = cursor_condition {
            qb.push(" AND (memorial_date, id) < (");
            qb.push_bind(date);
            qb.push(", ");
            qb.push_bind(id);
            qb.push(") ");
        }

        qb.push(" ORDER BY memorial_date DESC, id DESC ");
        qb.push(" LIMIT ");
        qb.push_bind(limit);

        let records = qb.build_query_as::<MemorialDay>().fetch_all(pool).await?;

        Ok((records, total))
    }

    pub async fn get_default_memorial_day(
        pool: &PgPool,
        group_id: i64,
    ) -> Result<MemorialDay, CustomError> {
        let record = sqlx::query_as::<_, MemorialDay>(
            "SELECT * FROM memorial_day WHERE group_id = $1 AND is_default = 1",
        )
        .bind(group_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| CustomError::not_found("默认纪念日不存在"))?;

        Ok(record)
    }
    
    pub async fn create_memorial_day(
        pool: &PgPool,
        data: MemorialDayCreate,
    ) -> Result<MemorialDay, CustomError> {
        let mut tx = pool.begin().await?;

        if let Some(1) = data.is_default {
            Self::reset_defaults(&mut tx, data.group_id).await?;
        }

        let record = sqlx::query_as::<_, MemorialDay>(
            "INSERT INTO memorial_day (group_id, name, description, memorial_date, is_default)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING *",
        )
        .bind(data.group_id)
        .bind(data.name)
        .bind(data.description)
        .bind(data.memorial_date)
        .bind(data.is_default.unwrap_or(0))
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(record)
    }

    pub async fn update_memorial_day(
        pool: &PgPool,
        id: i64,
        group_id: i64,
        data: MemorialDayUpdate,
    ) -> Result<MemorialDay, CustomError> {
        let mut tx = pool.begin().await?;

        // Verify existence and ownership
        let _current = sqlx::query_as::<_, MemorialDay>(
            "SELECT * FROM memorial_day WHERE id = $1 AND group_id = $2",
        )
        .bind(id)
        .bind(group_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::not_found("纪念日不存在"))?;

        if let Some(1) = data.is_default {
            Self::reset_defaults(&mut tx, group_id).await?;
        }

        let record = sqlx::query_as::<_, MemorialDay>(
            "UPDATE memorial_day
             SET name = COALESCE($1, name),
                 description = COALESCE($2, description),
                 memorial_date = COALESCE($3, memorial_date),
                 is_default = COALESCE($4, is_default),
                 updated_at = NOW()
             WHERE id = $5 AND group_id = $6
             RETURNING *",
        )
        .bind(data.name)
        .bind(data.description)
        .bind(data.memorial_date)
        .bind(data.is_default)
        .bind(id)
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(record)
    }

    pub async fn delete_memorial_day(
        pool: &PgPool,
        id: i64,
        group_id: i64,
    ) -> Result<(), CustomError> {
        let result = sqlx::query("DELETE FROM memorial_day WHERE id = $1 AND group_id = $2")
            .bind(id)
            .bind(group_id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(CustomError::not_found("纪念日不存在"));
        }

        Ok(())
    }

    async fn reset_defaults(
        tx: &mut Transaction<'_, Postgres>,
        group_id: i64,
    ) -> Result<(), CustomError> {
        sqlx::query(
            "UPDATE memorial_day SET is_default = 0 WHERE group_id = $1 AND is_default = 1",
        )
        .bind(group_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}
