//! 幂等键工具(FSD §3.3)
//!
//! 业务规则:写接口(POST/PUT/PATCH/DELETE)必须支持 `Idempotency-Key` Header。
//! 同一 `(user_id, method, route, key)` 24h 内的多次请求返回首次响应,不再重复执行 handler。
//!
//! 实现方式:工具函数(非中间件,避免与 ntex 响应体拦截耦合)
//! - handler 入口调用 `lookup_or_skip` 检查缓存
//! - 命中 → 直接返回缓存;未命中 → handler 继续执行
//! - handler 成功执行后调 `store` 回写缓存
//!
//! 用法示例:
//! ```ignore
//! pub async fn handler(state: ..., idem: IdempotencyKey, ...) -> ... {
//!     if let Some(cached) = idem::lookup_or_skip(&state, user_id, "POST", "/api/...", &idem.0).await? {
//!         return Ok(cached);
//!     }
//!     // 业务逻辑...
//!     let body = serde_json::json!({...});
//!     idem::store_response(&state, user_id, "POST", "/api/...", &idem.0, &body).await;
//!     Ok(body)
//! }
//! ```

use std::{future::Future, pin::Pin, sync::Arc};

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::{cache::RedisCache, errors::CustomError};

/// 缓存中的幂等响应体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedIdempotentResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

/// 构造缓存 key(组合 user_id + method + path + idempotency_key)
fn cache_key(user_id: i64, method: &str, path: &str, key: &str) -> String {
    format!(
        "idem:{}:{}:{}:{}",
        user_id,
        method.to_uppercase(),
        path.replace('/', "_"),
        key
    )
}

/// 查缓存。未命中返回 Ok(None),handler 继续执行。
pub async fn lookup_or_skip(
    redis: &Arc<RedisCache>,
    user_id: i64,
    method: &str,
    path: &str,
    key: Option<&str>,
) -> Result<Option<serde_json::Value>, CustomError> {
    let key = match key {
        Some(k) => k,
        None => return Ok(None), // 无 key,跳过
    };
    validate_idempotency_key(key)?;

    let ck = cache_key(user_id, method, path, key);
    let mut conn = redis
        .get_conn()
        .await
        .map_err(|e| CustomError::internal(format!("Redis 连接失败: {e}")))?;
    let body: Option<String> = conn
        .get(&ck)
        .await
        .map_err(|e| CustomError::internal(format!("Redis 读取失败: {e}")))?;

    match body {
        Some(s) => {
            let cached: CachedIdempotentResponse = serde_json::from_str(&s)
                .map_err(|e| CustomError::internal(format!("幂等缓存反序列化失败: {e}")))?;
            Ok(Some(cached.body))
        }
        None => Ok(None),
    }
}

/// 写缓存。handler 成功执行后调用。
/// 失败不回滚业务(缓存写失败不应阻塞用户操作,只 log 即可)。
pub async fn store_response(
    redis: &Arc<RedisCache>,
    user_id: i64,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: &serde_json::Value,
) {
    let key = match key {
        Some(k) => k,
        None => return,
    };
    let ck = cache_key(user_id, method, path, key);
    let entry = CachedIdempotentResponse {
        status: 200,
        body: body.clone(),
    };
    let s = match serde_json::to_string(&entry) {
        Ok(s) => s,
        Err(e) => {
            log::error!("幂等缓存序列化失败: {}", e);
            return;
        }
    };
    let mut conn = match redis.get_conn().await {
        Ok(c) => c,
        Err(e) => {
            log::error!("幂等缓存连接失败: {}", e);
            return;
        }
    };
    if let Err(e) = conn.set_ex::<_, _, ()>(&ck, s, 86400).await {
        log::error!("幂等缓存写入失败: {}", e);
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
