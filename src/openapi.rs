// OpenAPI 文档生成
// 占位符实现

use ntex::web::{HttpRequest, HttpResponse};
use std::sync::Arc;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi()]
pub struct ApiDoc;

pub async fn openapi_json() -> HttpResponse {
    HttpResponse::Ok().json(&serde_json::json!({}))
}

pub async fn serve_swagger(_req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().content_type("text/html").body("<html></html>")
}