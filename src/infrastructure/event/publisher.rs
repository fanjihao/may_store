// 基础设施层 - 事件发布器实现
// 负责将领域事件发布到 event_log 表

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::event::EventType;
use crate::infrastructure::event::log::EventLogQuery;
use crate::errors::CustomError;

/// 事件发布器
pub struct EventPublisher;

impl EventPublisher {
    /// 发布事件到 event_log 表
    pub async fn publish<T: serde::Serialize>(
        db: &PgPool,
        event_type: EventType,
        payload: T,
        user_id: Option<i64>,
        group_id: Option<i64>,
        ref_type: Option<&str>,
        ref_id: Option<i64>,
    ) -> Result<i64, CustomError> {
        let payload_json = serde_json::to_value(payload)
            .map_err(|e| CustomError::internal(format!("Failed to serialize payload: {}", e)))?;

        let idempotency_key = Self::generate_idempotency_key(
            event_type.as_str(),
            ref_type,
            ref_id,
        );

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO event_log (event_type, payload, user_id, group_id, ref_type, ref_id, idempotency_key)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(event_type.as_str())
        .bind(payload_json)
        .bind(user_id)
        .bind(group_id)
        .bind(ref_type)
        .bind(ref_id)
        .bind(idempotency_key)
        .fetch_one(db)
        .await
        .map_err(|e| CustomError::internal(format!("Failed to publish event: {}", e)))?;

        Ok(id)
    }

    /// 如果不存在则发布事件（幂等发布）
    pub async fn publish_if_not_exists<T: serde::Serialize>(
        db: &PgPool,
        event_type: EventType,
        payload: T,
        user_id: Option<i64>,
        group_id: Option<i64>,
        ref_type: Option<&str>,
        ref_id: Option<i64>,
    ) -> Result<Option<i64>, CustomError> {
        let idempotency_key = Self::generate_idempotency_key(
            event_type.as_str(),
            ref_type,
            ref_id,
        );

        if let Some(_) = EventLogQuery::find_by_idempotency_key(db, &idempotency_key).await? {
            return Ok(None);
        }

        let id = Self::publish(db, event_type, payload, user_id, group_id, ref_type, ref_id).await?;
        Ok(Some(id))
    }

    /// 生成幂等键
    fn generate_idempotency_key(
        event_type: &str,
        ref_type: Option<&str>,
        ref_id: Option<i64>,
    ) -> String {
        match (ref_type, ref_id) {
            (Some(rt), Some(rid)) => format!("{}:{}:{}", event_type, rt, rid),
            _ => format!("{}:{}", event_type, Uuid::new_v4()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idempotency_key_generation() {
        let key1 = EventPublisher::generate_idempotency_key("OrderCreatedEvent", Some("order"), Some(123));
        let key2 = EventPublisher::generate_idempotency_key("OrderCreatedEvent", Some("order"), Some(123));
        let key3 = EventPublisher::generate_idempotency_key("OrderCreatedEvent", Some("order"), Some(456));

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}
