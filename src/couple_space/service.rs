use super::models::{MemorialDay, MemorialDayCreate, MemorialDayUpdate};
use crate::errors::CustomError;
use chinese_lunisolar_calendar::LunisolarDate;
use chrono::{Datelike, NaiveDate};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};

pub struct MemorialDayService;

impl MemorialDayService {
    /// 计算纪念日的下一次公历日期
    pub fn get_next_occurrence(day: &MemorialDay, today: NaiveDate) -> NaiveDate {
        if day.calendar_type == "LUNAR" {
            let month = day.lunar_month.unwrap_or(1) as u8;
            let lunar_day = day.lunar_day.unwrap_or(1) as u8;
            let is_leap = day.is_leap_month.unwrap_or(false);

            // 尝试当前年份
            if let Ok(lunar) =
                LunisolarDate::from_ymd(today.year() as u16, month, is_leap, lunar_day)
            {
                let solar = lunar.to_solar_date().to_naive_date();
                if solar >= today {
                    return solar;
                }
            }

            // 尝试下一年
            if let Ok(lunar) =
                LunisolarDate::from_ymd(today.year() as u16 + 1, month, is_leap, lunar_day)
            {
                return lunar.to_solar_date().to_naive_date();
            }

            day.memorial_date // 降级返回存储日期
        } else {
            // SOLAR 公历逻辑
            let mut next = NaiveDate::from_ymd_opt(
                today.year(),
                day.memorial_date.month(),
                day.memorial_date.day(),
            )
            .unwrap_or_else(|| {
                // 处理 2-29 等特殊日期，退回到 2-28
                NaiveDate::from_ymd_opt(today.year(), day.memorial_date.month(), 28).unwrap()
            });

            if next < today {
                next = NaiveDate::from_ymd_opt(
                    today.year() + 1,
                    day.memorial_date.month(),
                    day.memorial_date.day(),
                )
                .unwrap_or_else(|| {
                    NaiveDate::from_ymd_opt(today.year() + 1, day.memorial_date.month(), 28)
                        .unwrap()
                });
            }
            next
        }
    }

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

        let mut records = qb.build_query_as::<MemorialDay>().fetch_all(pool).await?;
        let today = chrono::Utc::now().naive_local().date();

        for record in &mut records {
            let next = Self::get_next_occurrence(record, today);
            record.next_date = Some(next);
            record.days_remaining = Some((next - today).num_days());
        }

        Ok((records, total))
    }

    pub async fn get_default_memorial_day(
        pool: &PgPool,
        group_id: i64,
    ) -> Result<MemorialDay, CustomError> {
        let mut record = sqlx::query_as::<_, MemorialDay>(
            "SELECT * FROM memorial_day WHERE group_id = $1 AND is_default = 1",
        )
        .bind(group_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| CustomError::not_found("默认纪念日不存在"))?;

        let today = chrono::Utc::now().naive_local().date();
        let next = Self::get_next_occurrence(&record, today);
        record.next_date = Some(next);
        record.days_remaining = Some((next - today).num_days());

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

        let mut record = sqlx::query_as::<_, MemorialDay>(
            "INSERT INTO memorial_day (
                group_id, name, description, memorial_date,
                calendar_type, lunar_month, lunar_day, is_leap_month, is_default
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING *",
        )
        .bind(data.group_id)
        .bind(data.name)
        .bind(data.description)
        .bind(data.memorial_date)
        .bind(data.calendar_type.unwrap_or_else(|| "SOLAR".to_string()))
        .bind(data.lunar_month)
        .bind(data.lunar_day)
        .bind(data.is_leap_month.unwrap_or(false))
        .bind(data.is_default.unwrap_or(0))
        .fetch_one(&mut *tx)
        .await?;

        let today = chrono::Utc::now().naive_local().date();
        let next = Self::get_next_occurrence(&record, today);
        record.next_date = Some(next);
        record.days_remaining = Some((next - today).num_days());

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

        let mut record = sqlx::query_as::<_, MemorialDay>(
            "UPDATE memorial_day
             SET name = COALESCE($1, name),
                 description = COALESCE($2, description),
                 memorial_date = COALESCE($3, memorial_date),
                 calendar_type = COALESCE($4, calendar_type),
                 lunar_month = COALESCE($5, lunar_month),
                 lunar_day = COALESCE($6, lunar_day),
                 is_leap_month = COALESCE($7, is_leap_month),
                 is_default = COALESCE($8, is_default),
                 updated_at = NOW()
             WHERE id = $9 AND group_id = $10
             RETURNING *",
        )
        .bind(data.name)
        .bind(data.description)
        .bind(data.memorial_date)
        .bind(data.calendar_type)
        .bind(data.lunar_month)
        .bind(data.lunar_day)
        .bind(data.is_leap_month)
        .bind(data.is_default)
        .bind(id)
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;

        let today = chrono::Utc::now().naive_local().date();
        let next = Self::get_next_occurrence(&record, today);
        record.next_date = Some(next);
        record.days_remaining = Some((next - today).num_days());

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
