// API 层 - Swagger 文档路由
// 提供 OpenAPI JSON 文档和 Swagger UI 界面

use ntex::web::{self, HttpResponse, Responder, ServiceConfig};

/// 配置 Swagger 文档路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/swagger")
            .route("/openapi.json", web::get().to(get_openapi_json))
            .route("/index.html", web::get().to(get_swagger_ui)),
    );
}

/// 获取 OpenAPI JSON 文档
async fn get_openapi_json() -> impl Responder {
    let json = crate::openapi::openapi_json();
    HttpResponse::Ok()
        .content_type("application/json")
        .body(json)
}

/// 获取 Swagger UI 首页
async fn get_swagger_ui() -> impl Responder {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>Wish Menu API Docs</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui@5.9.0/dist/swagger-ui.css" />
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui@5.9.0/dist/swagger-ui-bundle.js"></script>
    <script>
        window.onload = function() {
            SwaggerUIBundle({
                url: "/swagger/openapi.json",
                dom_id: '#swagger-ui',
                layout: "BaseLayout"
            });
        };
    </script>
</body>
</html>
    "#;
    HttpResponse::Ok()
        .content_type("text/html")
        .body(html)
}
