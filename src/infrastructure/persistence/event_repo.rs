// 基础设施层 - PostgreSQL 事件仓储实现
// 实现 domain::event::EventRepository trait

use sqlx::{PgPool, Row};
use crate::domain::event::{EventRepository, EventLogRecord, EventType};
use crate::errors::CustomError;

/// PostgreSQL 事件仓储
#[allow(dead_code)]
pub struct PostgresEventRepository {
    pool: PgPool,
}

#[allow(dead_code)]
impl PostgresEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl EventRepository for PostgresEventRepository {
    async fn save(&self, event: &EventLogRecord) -> Result<i64, CustomError> {
        let id: i64 = sqlx::query_scalar(
            r#"INSERT INTO event_log (event_type, payload, status, idempotency_key)
               VALUES ($1, $2, $3::event_status_enum, $4) RETURNING id"#
        )
        .bind(serde_json::to_string(&event.event_type).unwrap_or_default())
        .bind(serde_json::to_value(&event.payload).unwrap_or(serde_json::json!({})))
        .bind(&event.status)
        .bind(&event.idempotency_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn find_by_idempotency_key(&self, key: &str) -> Result<Option<EventLogRecord>, CustomError> {
        let row = sqlx::query(
            r#"SELECT id, event_type, payload, status, idempotency_key, created_at, processed_at
               FROM event_log WHERE idempotency_key = $1"#
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            let event_type_str: String = r.get("event_type");
            let event_type = serde_json::from_str(&event_type_str).unwrap_or(EventType::Unknown);
            let payload: serde_json::Value = r.get("payload");
            let status: String = r.get("status");
            let idempotency_key: Option<String> = r.get("idempotency_key");
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            let processed_at: Option<chrono::DateTime<chrono::Utc>> = r.get("processed_at");

            Ok(Some(EventLogRecord {
                id: r.get("id"),
                event_type,
                payload,
                status,
                idempotency_key,
                created_at,
                processed_at,
            }))
        } else {
            Ok(None)
        }
    }

    async fn find_pending(&self, limit: i64) -> Result<Vec<EventLogRecord>, CustomError> {
        let rows = sqlx::query(
            r#"SELECT id, event_type, payload, status, idempotency_key, created_at, processed_at
               FROM event_log WHERE status = 'PENDING'::event_status_enum ORDER BY created_at ASC LIMIT $1"#
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let events: Vec<EventLogRecord> = rows.into_iter().map(|r| {
            let event_type_str: String = r.get("event_type");
            let event_type = serde_json::from_str(&event_type_str).unwrap_or(EventType::Unknown);
            let payload: serde_json::Value = r.get("payload");
            let status: String = r.get("status");
            let idempotency_key: Option<String> = r.get("idempotency_key");
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            let processed_at: Option<chrono::DateTime<chrono::Utc>> = r.get("processed_at");

            EventLogRecord {
                id: r.get("id"),
                event_type,
                payload,
                status,
                idempotency_key,
                created_at,
                processed_at,
            }
        }).collect();

        Ok(events)
    }

    async fn update_status(&self, id: i64, status: &str) -> Result<(), CustomError> {
        let processed_at = if status == "DONE" || status == "FAILED" {
            Some(chrono::Utc::now())
        } else {
            None
        };

        sqlx::query(
            r#"UPDATE event_log SET status = $2::event_status_enum, processed_at = $3 WHERE id = $1"#
        )
        .bind(id)
        .bind(status)
        .bind(processed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
