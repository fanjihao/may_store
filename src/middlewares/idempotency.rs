//! 原子幂等键工具(FSD §3.3)。
//!
//! 有 `Idempotency-Key` 时先以 Redis `SET NX EX` 抢占短期 PENDING reservation。
//! 只有 owner 可以通过 Lua 将其完成或删除；并发请求短暂等待，随后重放
//! COMPLETED payload 或返回明确冲突。无 key 时保持旧接口兼容，直接执行业务。

use std::{future::Future, sync::Arc, time::Duration};

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{cache::RedisCache, errors::CustomError};

const PENDING_TTL_SECONDS: usize = 30;
const COMPLETED_TTL_SECONDS: usize = 24 * 60 * 60;
const PENDING_WAIT_ATTEMPTS: usize = 5;
const PENDING_WAIT_INTERVAL_MS: u64 = 50;

const COMPLETE_IF_OWNER_LUA: &str = r#"
local current = redis.call('GET', KEYS[1])
if current == ARGV[1] then
    redis.call('SET', KEYS[1], ARGV[2], 'EX', ARGV[3])
    return 1
end
return 0
"#;

const DELETE_IF_OWNER_LUA: &str = r#"
local current = redis.call('GET', KEYS[1])
if current == ARGV[1] then
    return redis.call('DEL', KEYS[1])
end
return 0
"#;

/// 缓存中的成功响应 data payload。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedIdempotentResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "SCREAMING_SNAKE_CASE")]
enum StoredReservation {
    Pending {
        owner_token: String,
    },
    Completed {
        status: u16,
        body: serde_json::Value,
    },
}

/// handler 在执行业务前得到的原子 reservation 结果。
#[derive(Debug)]
pub enum ReservationOutcome {
    /// 调用方未提供 key，保持兼容并直接执行业务。
    Bypass,
    /// 当前请求获得唯一执行权。
    Acquired(IdempotencyReservation),
    /// 首次请求已完成，直接返回其 data payload。
    Completed(CachedIdempotentResponse),
}

/// Redis reservation 的 owner 句柄。
#[derive(Debug)]
pub struct IdempotencyReservation {
    redis: Arc<RedisCache>,
    cache_key: String,
    owner_token: String,
}

fn cache_key(user_id: i64, method: &str, path: &str, key: &str) -> String {
    format!(
        "idem:{}:{}:{}:{}",
        user_id,
        method.to_uppercase(),
        path,
        key
    )
}

fn serialize_record(record: &StoredReservation) -> Result<String, CustomError> {
    serde_json::to_string(record)
        .map_err(|e| CustomError::internal(format!("幂等状态序列化失败: {e}")))
}

fn deserialize_record(raw: &str) -> Result<StoredReservation, CustomError> {
    serde_json::from_str(raw)
        .map_err(|e| CustomError::internal(format!("幂等状态反序列化失败: {e}")))
}

fn redis_error(operation: &str, error: redis::RedisError) -> CustomError {
    CustomError::internal(format!("Redis {operation}失败: {error}"))
}

/// 原子抢占幂等执行权。
pub async fn reserve(
    redis: &Arc<RedisCache>,
    user_id: i64,
    method: &str,
    path: &str,
    key: Option<&str>,
) -> Result<ReservationOutcome, CustomError> {
    let Some(key) = key else {
        return Ok(ReservationOutcome::Bypass);
    };
    validate_idempotency_key(key)?;

    let cache_key = cache_key(user_id, method, path, key);
    let owner_token = Uuid::new_v4().to_string();
    let pending = serialize_record(&StoredReservation::Pending {
        owner_token: owner_token.clone(),
    })?;
    let mut conn = redis.get_conn().await.map_err(|e| redis_error("连接", e))?;

    for attempt in 0..=PENDING_WAIT_ATTEMPTS {
        let acquired: Option<String> = redis::cmd("SET")
            .arg(&cache_key)
            .arg(&pending)
            .arg("NX")
            .arg("EX")
            .arg(PENDING_TTL_SECONDS)
            .query_async(&mut conn)
            .await
            .map_err(|e| redis_error("预占", e))?;

        if acquired.is_some() {
            return Ok(ReservationOutcome::Acquired(IdempotencyReservation {
                redis: Arc::clone(redis),
                cache_key,
                owner_token,
            }));
        }

        let current: Option<String> = conn
            .get(&cache_key)
            .await
            .map_err(|e| redis_error("读取", e))?;
        match current.as_deref().map(deserialize_record).transpose()? {
            Some(StoredReservation::Completed { status, body }) => {
                return Ok(ReservationOutcome::Completed(CachedIdempotentResponse {
                    status,
                    body,
                }));
            }
            Some(StoredReservation::Pending { .. }) => {
                if attempt == PENDING_WAIT_ATTEMPTS {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(PENDING_WAIT_INTERVAL_MS)).await;
            }
            None => {
                // PENDING 可能恰好过期；下一轮重新尝试 SET NX。
            }
        }
    }

    Err(CustomError::idempotency_conflict(
        "相同 Idempotency-Key 的请求正在处理中，请稍后重试",
    ))
}

impl IdempotencyReservation {
    fn pending_record(&self) -> Result<String, CustomError> {
        serialize_record(&StoredReservation::Pending {
            owner_token: self.owner_token.clone(),
        })
    }

    /// 仅 owner 可把 PENDING 原子替换为 24h COMPLETED payload。
    pub async fn complete(&self, status: u16, body: &serde_json::Value) -> Result<(), CustomError> {
        let expected_pending = self.pending_record()?;
        let completed = serialize_record(&StoredReservation::Completed {
            status,
            body: body.clone(),
        })?;
        let mut conn = self
            .redis
            .get_conn()
            .await
            .map_err(|e| redis_error("连接", e))?;
        let replaced: i64 = redis::Script::new(COMPLETE_IF_OWNER_LUA)
            .key(&self.cache_key)
            .arg(expected_pending)
            .arg(completed)
            .arg(COMPLETED_TTL_SECONDS)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| redis_error("完成幂等预占", e))?;

        if replaced == 1 {
            Ok(())
        } else {
            Err(CustomError::idempotency_conflict(
                "幂等执行权已失效，无法安全保存响应",
            ))
        }
    }

    /// 业务失败时仅由 owner compare-delete，绝不删除其他请求的新 reservation。
    pub async fn release(&self) {
        if let Err(error) = self.release_inner().await {
            log::error!("释放幂等预占失败: {}", error.message());
        }
    }

    async fn release_inner(&self) -> Result<(), CustomError> {
        let expected_pending = self.pending_record()?;
        let mut conn = self
            .redis
            .get_conn()
            .await
            .map_err(|e| redis_error("连接", e))?;
        let _: i64 = redis::Script::new(DELETE_IF_OWNER_LUA)
            .key(&self.cache_key)
            .arg(expected_pending)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| redis_error("释放幂等预占", e))?;
        Ok(())
    }
}

/// 验证:业务写接口的 Idempotency-Key 头
/// - 长度 8-128
/// - 仅含 ASCII 可打印字符
pub fn validate_idempotency_key(key: &str) -> Result<(), CustomError> {
    if key.len() < 8 || key.len() > 128 {
        return Err(CustomError::BadRequest(
            "Idempotency-Key 长度必须在 8-128 字符之间".into(),
        ));
    }
    if !key.chars().all(|c| c.is_ascii_graphic()) {
        return Err(CustomError::BadRequest(
            "Idempotency-Key 只能包含 ASCII 可打印字符".into(),
        ));
    }
    Ok(())
}

/// FromRequest 提取器:handler 加 `idem: IdempotencyKey` 即可获得键值
#[derive(Debug, Clone)]
pub struct IdempotencyKey(pub Option<String>);

impl<E: ErrorRenderer> FromRequest<E> for IdempotencyKey {
    type Error = CustomError;

    fn from_request(
        req: &HttpRequest,
        _: &mut Payload,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        let key = req
            .headers()
            .get("Idempotency-Key")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        async move { Ok(IdempotencyKey(key)) }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier, Mutex};

    use super::{
        cache_key, deserialize_record, serialize_record, StoredReservation, PENDING_WAIT_ATTEMPTS,
    };

    fn reserve_in_memory(state: &Mutex<Option<StoredReservation>>, owner_token: String) -> bool {
        let mut state = state.lock().expect("reservation state poisoned");
        if state.is_some() {
            return false;
        }
        *state = Some(StoredReservation::Pending { owner_token });
        true
    }

    fn release_in_memory(state: &Mutex<Option<StoredReservation>>, owner_token: &str) -> bool {
        let mut state = state.lock().expect("reservation state poisoned");
        match state.as_ref() {
            Some(StoredReservation::Pending {
                owner_token: current,
            }) if current == owner_token => {
                *state = None;
                true
            }
            _ => false,
        }
    }

    #[test]
    fn completed_state_round_trips_data_payload() {
        let expected = StoredReservation::Completed {
            status: 200,
            body: serde_json::json!({"wishId": 42}),
        };
        let encoded = serialize_record(&expected).unwrap();
        assert_eq!(deserialize_record(&encoded).unwrap(), expected);
    }

    #[test]
    fn cache_key_is_scoped_to_actual_resource_path() {
        let first = cache_key(7, "post", "/api/wishes/11/select", "request-key");
        let retry = cache_key(7, "POST", "/api/wishes/11/select", "request-key");
        let other_wish = cache_key(7, "POST", "/api/wishes/12/select", "request-key");
        assert_eq!(first, retry);
        assert_ne!(first, other_wish);
    }

    #[test]
    fn concurrent_reservation_has_exactly_one_owner() {
        const WORKERS: usize = 16;
        let state = Arc::new(Mutex::new(None));
        let barrier = Arc::new(Barrier::new(WORKERS));
        let handles: Vec<_> = (0..WORKERS)
            .map(|worker| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    reserve_in_memory(&state, format!("owner-{worker}"))
                })
            })
            .collect();

        let winners = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
    }

    #[test]
    fn only_owner_can_release_pending_state() {
        let state = Mutex::new(Some(StoredReservation::Pending {
            owner_token: "owner-a".into(),
        }));
        assert!(!release_in_memory(&state, "owner-b"));
        assert!(state.lock().unwrap().is_some());
        assert!(release_in_memory(&state, "owner-a"));
        assert!(state.lock().unwrap().is_none());
    }

    #[test]
    fn pending_wait_is_bounded_before_conflict() {
        assert!(PENDING_WAIT_ATTEMPTS > 0);
        assert!(PENDING_WAIT_ATTEMPTS <= 10);
    }
}
