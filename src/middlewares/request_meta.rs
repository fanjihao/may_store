//! 请求元数据提取器
//!
//! `TraceId` —— handler 加 `trace_id: TraceId` 参数即可获得
//!   - 优先采用请求头 `X-Trace-Id`
//!   - 缺失则生成 UUID v4
//!
//! 响应头 `X-Trace-Id` 由客户端按需通过 middleware 添加(本工程不强制)
//! 或由反向代理层统一注入。

use std::time::Instant;

use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use uuid::Uuid;

/// 链路追踪 ID(从 extensions 中提取)
#[derive(Debug, Clone)]
pub struct TraceId(pub String);

impl<E: ErrorRenderer> FromRequest<E> for TraceId {
    type Error = ntex::web::Error;

    fn from_request(
        req: &HttpRequest,
        _: &mut Payload,
    ) -> impl std::future::Future<Output = Result<Self, Self::Error>> {
        let id = req
            .extensions()
            .get::<TraceId>()
            .cloned()
            .unwrap_or_else(|| {
                req.headers()
                    .get("X-Trace-Id")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| TraceId(s.to_string()))
                    .unwrap_or_else(|| TraceId(Uuid::new_v4().to_string()))
            });
        async move { Ok(id) }
    }
}

/// 公共请求开始时间(供后续 metrics 统计 P95)
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct RequestStart(pub Instant);
