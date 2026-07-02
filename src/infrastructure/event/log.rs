// 基础设施层 - 事件日志数据库实现
// 包含事件日志查询和状态管理

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use chrono::{DateTime, Utc};

use crate::domain::event::EventType;

/// 事件日志记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EventLog {
    pub id: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub user_id: Option<i64>,
    pub group_id: Option<i64>,
    pub ref_type: Option<String>,
    pub ref_id: Option<i64>,
    pub status: String,
    pub retry_count: i32,
    pub max_retries: i32,
    pub error_message: Option<String>,
    pub idempotency_key: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
}

#[allow(dead_code)]
impl EventLog {
    /// 获取事件类型枚举
    pub fn event_type_enum(&self) -> Option<EventType> {
        EventType::from_str(&self.event_type)
    }
}

/// 事件日志查询
#[allow(dead_code)]
pub struct EventLogQuery;

#[allow(dead_code)]
impl EventLogQuery {
    /// 获取待处理事件
    pub async fn fetch_pending_events(
        db: &PgPool,
        limit: i64,
    ) -> Result<Vec<EventLog>, sqlx::Error> {
        sqlx::query_as::<_, EventLog>(
            r#"
            SELECT id, event_type, payload, user_id, group_id, ref_type, ref_id,
                   status, retry_count, max_retries, error_message, idempotency_key,
                   trace_id, created_at, processed_at
            FROM event_log
            WHERE status = 'PENDING'::event_status_enum
            ORDER BY created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(limit)
        .fetch_all(db)
        .await
    }

    /// 标记为处理中
    pub async fn mark_processing(db: &PgPool, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE event_log
            SET status = 'PROCESSING'::event_status_enum
            WHERE id = $1 AND status = 'PENDING'::event_status_enum
            "#,
        )
        .bind(id)
        .execute(db)
        .await?;
        Ok(())
    }

    /// 标记为已完成
    pub async fn mark_done(db: &PgPool, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE event_log
            SET status = 'DONE'::event_status_enum, processed_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(db)
        .await?;
        Ok(())
    }

    /// 标记为失败（如果超过最大重试次数则标记为 FAILED，否则重置为 PENDING）
    pub async fn mark_failed(
        db: &PgPool,
        id: i64,
        error_message: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE event_log
            SET status = CASE
                WHEN retry_count + 1 >= max_retries THEN 'FAILED'
                ELSE 'PENDING'
                END,
                retry_count = retry_count + 1,
                error_message = $2
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(error_message)
        .execute(db)
        .await?;
        Ok(())
    }

    /// 根据幂等键查找事件
    pub async fn find_by_idempotency_key(
        db: &PgPool,
        key: &str,
    ) -> Result<Option<EventLog>, sqlx::Error> {
        sqlx::query_as::<_, EventLog>(
            r#"
            SELECT id, event_type, payload, user_id, group_id, ref_type, ref_id,
                   status, retry_count, max_retries, error_message, idempotency_key,
                   trace_id, created_at, processed_at
            FROM event_log
            WHERE idempotency_key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(db)
        .await
    }
}
