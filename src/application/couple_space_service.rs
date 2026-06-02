// 应用服务层 - 情侣空间服务

use chrono::NaiveDate;
use sqlx::{PgPool, Row};
use crate::domain::couple_space::{MemorialDay, MemorialDayCreate, MemorialDayQuery, MemorialDayUpdate};
use crate::errors::CustomError;

pub struct MemorialDayService;

#[allow(dead_code)]
impl MemorialDayService {
    /// 获取纪念日列表
    pub async fn list_memorial_days(
        db: &PgPool,
        user_id: i64,
        query: &MemorialDayQuery,
    ) -> Result<Vec<MemorialDay>, CustomError> {
        let limit = query.limit.unwrap_or(50).min(100);

        // 获取用户的情侣用户ID
        let couple_user_id: Option<i64> = sqlx::query(
            "SELECT partner_user_id FROM couple_relations WHERE user_id = $1 LIMIT 1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?
        .map(|r| r.get("partner_user_id"));

        let couple_uid = match couple_user_id {
            Some(id) => id,
            None => return Err(CustomError::NotFound("未找到情侣关系".into())),
        };

        let rows = sqlx::query(
            "SELECT id, user_id, couple_user_id, name, date, day_type, created_at, updated_at \
             FROM memorial_days \
             WHERE user_id = $1 OR user_id = $2 \
             ORDER BY date DESC \
             LIMIT $3"
        )
        .bind(user_id as i64)
        .bind(couple_uid)
        .bind(limit)
        .fetch_all(db)
        .await?;

        let days: Vec<MemorialDay> = rows.into_iter()
            .map(|r| MemorialDay {
                id: r.get("id"),
                user_id: r.get("user_id"),
                couple_user_id: r.get("couple_user_id"),
                name: r.get("name"),
                date: r.get("date"),
                day_type: r.get("day_type"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect();

        Ok(days)
    }

    /// 创建纪念日
    pub async fn create_memorial_day(
        db: &PgPool,
        user_id: i64,
        input: &MemorialDayCreate,
    ) -> Result<MemorialDay, CustomError> {
        // 获取用户的情侣用户ID
        let couple_user_id: Option<i64> = sqlx::query(
            "SELECT partner_user_id FROM couple_relations WHERE user_id = $1 LIMIT 1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?
        .map(|r| r.get("partner_user_id"));

        let couple_uid = match couple_user_id {
            Some(id) => id,
            None => return Err(CustomError::NotFound("未找到情侣关系".into())),
        };

        let day_type = input.day_type.clone().unwrap_or_else(|| "custom".to_string());

        let row = sqlx::query(
            "INSERT INTO memorial_days (user_id, couple_user_id, name, date, day_type) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, user_id, couple_user_id, name, date, day_type, created_at, updated_at"
        )
        .bind(user_id as i64)
        .bind(couple_uid)
        .bind(&input.name)
        .bind(input.date)
        .bind(&day_type)
        .fetch_one(db)
        .await?;

        Ok(MemorialDay {
            id: row.get("id"),
            user_id: row.get("user_id"),
            couple_user_id: row.get("couple_user_id"),
            name: row.get("name"),
            date: row.get("date"),
            day_type: row.get("day_type"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// 更新纪念日
    pub async fn update_memorial_day(
        db: &PgPool,
        user_id: i64,
        id: i64,
        input: &MemorialDayUpdate,
    ) -> Result<MemorialDay, CustomError> {
        // 检查权限
        let owner: Option<i64> = sqlx::query("SELECT user_id FROM memorial_days WHERE id = $1")
            .bind(id)
            .fetch_optional(db)
            .await?
            .map(|r| r.get("user_id"));

        if owner.map(|o| o != user_id as i64).unwrap_or(true) {
            return Err(CustomError::Forbidden("无权修改此纪念日".into()));
        }

        let row = sqlx::query(
            "UPDATE memorial_days SET \
             name = COALESCE($2, name), \
             date = COALESCE($3, date), \
             day_type = COALESCE($4, day_type) \
             WHERE id = $1 \
             RETURNING id, user_id, couple_user_id, name, date, day_type, created_at, updated_at"
        )
        .bind(id)
        .bind(&input.name)
        .bind(input.date)
        .bind(&input.day_type)
        .fetch_one(db)
        .await
        .map_err(|_| CustomError::NotFound("纪念日不存在".into()))?;

        Ok(MemorialDay {
            id: row.get("id"),
            user_id: row.get("user_id"),
            couple_user_id: row.get("couple_user_id"),
            name: row.get("name"),
            date: row.get("date"),
            day_type: row.get("day_type"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// 删除纪念日
    pub async fn delete_memorial_day(
        db: &PgPool,
        user_id: i64,
        id: i64,
    ) -> Result<(), CustomError> {
        // 检查权限
        let owner: Option<i64> = sqlx::query("SELECT user_id FROM memorial_days WHERE id = $1")
            .bind(id)
            .fetch_optional(db)
            .await?
            .map(|r| r.get("user_id"));

        if owner.map(|o| o != user_id as i64).unwrap_or(true) {
            return Err(CustomError::Forbidden("无权删除此纪念日".into()));
        }

        sqlx::query("DELETE FROM memorial_days WHERE id = $1")
            .bind(id)
            .execute(db)
            .await?;

        Ok(())
    }

    /// 获取默认纪念日（在一起的日子）
    pub async fn get_default_memorial_day(
        db: &PgPool,
        user_id: i64,
    ) -> Result<Option<MemorialDay>, CustomError> {
        // 获取用户的情侣用户ID和在一起日期
        let couple_info: Option<(i64, NaiveDate)> = sqlx::query_as(
            "SELECT partner_user_id, start_date FROM couple_relations WHERE user_id = $1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?;

        let (couple_uid, start_date) = match couple_info {
            Some((uid, date)) => (uid, date),
            None => return Ok(None),
        };

        // 查找是否已有"在一起"纪念日
        let existing_row = sqlx::query(
            "SELECT id, user_id, couple_user_id, name, date, day_type, created_at, updated_at \
             FROM memorial_days WHERE user_id = $1 AND day_type = 'anniversary' LIMIT 1"
        )
        .bind(user_id as i64)
        .fetch_optional(db)
        .await?;

        if let Some(row) = existing_row {
            return Ok(Some(MemorialDay {
                id: row.get("id"),
                user_id: row.get("user_id"),
                couple_user_id: row.get("couple_user_id"),
                name: row.get("name"),
                date: row.get("date"),
                day_type: row.get("day_type"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }));
        }

        // 创建默认纪念日
        let row = sqlx::query(
            "INSERT INTO memorial_days (user_id, couple_user_id, name, date, day_type) \
             VALUES ($1, $2, '在一起', $3, 'anniversary') \
             RETURNING id, user_id, couple_user_id, name, date, day_type, created_at, updated_at"
        )
        .bind(user_id as i64)
        .bind(couple_uid)
        .bind(start_date)
        .fetch_optional(db)
        .await?;

        Ok(row.map(|r| MemorialDay {
            id: r.get("id"),
            user_id: r.get("user_id"),
            couple_user_id: r.get("couple_user_id"),
            name: r.get("name"),
            date: r.get("date"),
            day_type: r.get("day_type"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }
}
