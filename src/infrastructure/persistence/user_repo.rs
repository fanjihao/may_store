// 基础设施层 - PostgreSQL 用户仓储实现
// 实现 domain::user::UserRepository trait

use sqlx::{PgPool, Row};
use crate::domain::user::{UserRepository, UserRecord, UserUpdateData};
use crate::errors::CustomError;

/// PostgreSQL 用户仓储
#[allow(dead_code)]
pub struct PostgresUserRepository {
    pool: PgPool,
}

#[allow(dead_code)]
impl PostgresUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserRepository for PostgresUserRepository {
    async fn find_by_id(&self, user_id: i64) -> Result<Option<UserRecord>, CustomError> {
        let rec = sqlx::query_as::<_, UserRecord>(
            r#"SELECT user_id, username, nick_name, email, role, love_point, diamond, avatar, phone, open_id, status, created_at, updated_at, password_hash, password_algo, gender, birthday, username_change, login_method, last_login_at, password_updated_at, is_temp_password, push_id, last_role_switch_at
               FROM users WHERE user_id = $1"#
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rec)
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<UserRecord>, CustomError> {
        let rec = sqlx::query_as::<_, UserRecord>(
            r#"SELECT user_id, username, nick_name, email, role, love_point, diamond, avatar, phone, open_id, status, created_at, updated_at, password_hash, password_algo, gender, birthday, username_change, login_method, last_login_at, password_updated_at, is_temp_password, push_id, last_role_switch_at
               FROM users WHERE username = $1"#
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rec)
    }

    async fn find_by_open_id(&self, open_id: &str) -> Result<Option<UserRecord>, CustomError> {
        let rec = sqlx::query_as::<_, UserRecord>(
            r#"SELECT user_id, username, nick_name, email, role, love_point, diamond, avatar, phone, open_id, status, created_at, updated_at, password_hash, password_algo, gender, birthday, username_change, login_method, last_login_at, password_updated_at, is_temp_password, push_id, last_role_switch_at
               FROM users WHERE open_id = $1"#
        )
        .bind(open_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rec)
    }

    async fn save(&self, user: &UserRecord) -> Result<(), CustomError> {
        sqlx::query(
            r#"INSERT INTO users (user_id, username, nick_name, email, role, love_point, diamond, avatar, phone, open_id, is_active)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#
        )
        .bind(user.user_id)
        .bind(&user.username)
        .bind(&user.nick_name)
        .bind(&user.email)
        .bind(&user.role)
        .bind(user.love_point)
        .bind(user.diamond)
        .bind(&user.avatar)
        .bind(&user.phone)
        .bind(&user.open_id)
        .bind(user.is_active)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, user_id: i64, data: &UserUpdateData) -> Result<(), CustomError> {
        if let Some(nick_name) = &data.nick_name {
            sqlx::query("UPDATE users SET nick_name = $2 WHERE user_id = $1")
                .bind(user_id)
                .bind(nick_name)
                .execute(&self.pool)
                .await?;
        }
        if let Some(avatar) = &data.avatar {
            sqlx::query("UPDATE users SET avatar = $2 WHERE user_id = $1")
                .bind(user_id)
                .bind(avatar)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn exists_by_username(&self, username: &str) -> Result<bool, CustomError> {
        let row = sqlx::query("SELECT COUNT(*) FROM users WHERE username = $1")
            .bind(username)
            .fetch_one(&self.pool)
            .await?;
        let count: i64 = row.get(0);
        Ok(count > 0)
    }
}
