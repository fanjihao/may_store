// 领域层 - 事件仓储 trait
// 定义事件日志的持久化接口

use crate::errors::CustomError;
use super::types::EventType;

/// 事件日志记录
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EventLogRecord {
    pub id: i64,
    pub event_type: EventType,
    pub payload: serde_json::Value,
    pub status: String,
    pub idempotency_key: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 事件仓储接口
#[allow(dead_code)]
pub trait EventRepository: Send + Sync {
    /// 保存事件日志
    async fn save(&self, event: &EventLogRecord) -> Result<i64, CustomError>;

    /// 根据幂等key查询事件
    async fn find_by_idempotency_key(&self, key: &str) -> Result<Option<EventLogRecord>, CustomError>;

    /// 查询待处理事件
    async fn find_pending(&self, limit: i64) -> Result<Vec<EventLogRecord>, CustomError>;

    /// 更新事件状态
    async fn update_status(&self, id: i64, status: &str) -> Result<(), CustomError>;
}
