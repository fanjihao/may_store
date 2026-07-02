// 应用服务层 - 情侣空间服务
// Note: This service is designed for a user-couple based model, but v3.sql uses a group-based model
// The memorial_day table in v3.sql has: id, group_id, name, description, memorial_date, etc.
// This service references: user_id, couple_user_id, name, date, day_type which don't match

use crate::domain::couple_space::{
    MemorialDay, MemorialDayCreate, MemorialDayQuery, MemorialDayUpdate,
};
use crate::errors::CustomError;
use sqlx::{PgPool, Row};

pub struct MemorialDayService;

#[allow(dead_code)]
impl MemorialDayService {
    /// 获取纪念日列表 - Uses correct memorial_day columns from v3.sql
    pub async fn list_memorial_days(
        db: &PgPool,
        user_id: i64,
        query: &MemorialDayQuery,
    ) -> Result<Vec<MemorialDay>, CustomError> {
        let limit = query.limit.unwrap_or(50).min(100);

        // Get user's group_id
        let group_id: Option<i64> = sqlx::query_scalar(
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND member_status='ACTIVE'::group_member_status_enum LIMIT 1"
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        let gid = match group_id {
            Some(id) => id,
            None => return Err(CustomError::NotFound("用户不在任何组中".into())),
        };

        let rows = sqlx::query(
            "SELECT id, group_id, name, description, memorial_date, calendar_type, created_at, updated_at \
             FROM memorial_day \
             WHERE group_id = $1 \
             ORDER BY memorial_date DESC \
             LIMIT $2",
        )
        .bind(gid)
        .bind(limit)
        .fetch_all(db)
        .await?;

        let days: Vec<MemorialDay> = rows
            .into_iter()
            .map(|r| MemorialDay {
                id: r.get("id"),
                user_id: r.get("group_id"), // map group_id to user_id for compatibility
                couple_user_id: 0,
                name: r.get("name"),
                date: r.get("memorial_date"), // map memorial_date to date
                day_type: r.get::<Option<String>, _>("calendar_type").unwrap_or_else(|| "SOLAR".to_string()),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect();

        Ok(days)
    }

    /// 创建纪念日 - Uses correct memorial_day columns from v3.sql
    pub async fn create_memorial_day(
        db: &PgPool,
        user_id: i64,
        input: &MemorialDayCreate,
    ) -> Result<MemorialDay, CustomError> {
        // Get user's group_id
        let group_id: Option<i64> = sqlx::query_scalar(
            "SELECT group_id FROM association_group_members WHERE user_id = $1 AND member_status='ACTIVE'::group_member_status_enum LIMIT 1"
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        let gid = match group_id {
            Some(id) => id,
            None => return Err(CustomError::NotFound("用户不在任何组中".into())),
        };

        let row = sqlx::query(
            "INSERT INTO memorial_day (group_id, name, memorial_date, calendar_type) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, group_id, name, description, memorial_date, calendar_type, created_at, updated_at",
        )
        .bind(gid)
        .bind(&input.name)
        .bind(input.date)
        .bind(input.day_type.as_deref().unwrap_or("SOLAR"))
        .fetch_one(db)
        .await?;

        Ok(MemorialDay {
            id: row.get("id"),
            user_id: row.get("group_id"),
            couple_user_id: 0,
            name: row.get("name"),
            date: row.get("memorial_date"),
            day_type: row.get::<Option<String>, _>("calendar_type").unwrap_or_else(|| "SOLAR".to_string()),
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
        // 检查权限 - user must be member of the group that owns this memorial
        let owner: Option<i64> = sqlx::query_scalar(
            "SELECT group_id FROM memorial_day WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(db)
        .await?;

        let owner_group_id = match owner {
            Some(o) => o,
            None => return Err(CustomError::NotFound("纪念日不存在".into())),
        };

        // Check user is member of this group
        let is_member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)"
        )
        .bind(owner_group_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        if !is_member {
            return Err(CustomError::Forbidden("无权修改此纪念日".into()));
        }

        let row = sqlx::query(
            "UPDATE memorial_day SET \
             name = COALESCE($2, name), \
             memorial_date = COALESCE($3, memorial_date), \
             calendar_type = COALESCE($4, calendar_type) \
             WHERE id = $1 \
             RETURNING id, group_id, name, description, memorial_date, calendar_type, created_at, updated_at",
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
            user_id: row.get("group_id"),
            couple_user_id: 0,
            name: row.get("name"),
            date: row.get("memorial_date"),
            day_type: row.get::<Option<String>, _>("calendar_type").unwrap_or_else(|| "SOLAR".to_string()),
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
        let owner: Option<i64> = sqlx::query_scalar(
            "SELECT group_id FROM memorial_day WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(db)
        .await?;

        let owner_group_id = match owner {
            Some(o) => o,
            None => return Err(CustomError::NotFound("纪念日不存在".into())),
        };

        // Check user is member of this group
        let is_member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)"
        )
        .bind(owner_group_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        if !is_member {
            return Err(CustomError::Forbidden("无权删除此纪念日".into()));
        }

        sqlx::query("DELETE FROM memorial_day WHERE id = $1")
            .bind(id)
            .execute(db)
            .await?;

        Ok(())
    }

    /// 获取默认纪念日（在一起的日子）- Not applicable in group-based model
    #[allow(dead_code)]
    pub async fn get_default_memorial_day(
        _db: &PgPool,
        _user_id: i64,
    ) -> Result<Option<MemorialDay>, CustomError> {
        // In v3.sql, there's no couple_relations table, so we can't get "start_date"
        // The default memorial day concept doesn't translate to group-based model
        Ok(None)
    }
}
