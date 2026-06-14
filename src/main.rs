// 主入口文件
// 遵循 FSD (Feature-Sliced Design) 架构

#![allow(dead_code)]

mod api; // API 层 - HTTP 路由和处理器
mod application; // 应用服务层 - 业务用例编排
mod domain; // 领域层 - 核心业务逻辑
mod infrastructure; // 基础设施层 - 数据库、外部服务

// 保留通用模块
mod cache;
mod config;
mod errors;
mod middlewares;
mod models;
mod openapi;
mod utils;

use dotenvy::dotenv;
use errors::CustomError;
use idgenerator::{IdGeneratorOptions, IdInstance};
use ntex::web::{middleware, App, HttpServer};
use ntex_cors::Cors;
use std::{env, sync::Arc};

use config::init_app_state;
use middlewares::logger::init_logger;

#[ntex::main]
async fn main() -> Result<(), CustomError> {
    dotenv().ok();

    // 初始化日志
    init_logger();

    // 雪花 ID 生成器
    let options = IdGeneratorOptions::new().worker_id(1).worker_id_bit_len(6);
    let _ = IdInstance::init(options)?;

    // 应用状态
    let app_state = init_app_state().await?;

    // 启动 WebSocket 服务器（独立端口 9832）
    let app_state_for_ws = app_state.clone();
    let ws_handle = tokio::spawn(async move {
        use api::ws;
        if let Err(e) = ws::start_websocket_server("0.0.0.0:9832", app_state_for_ws).await {
            log::error!("WebSocket 服务器错误: {}", e);
        }
    });

    let allowed_origin = env::var("FRONTEND_ORIGIN").unwrap_or_else(|_| "*".to_string());

    // 生产环境提示:不要省略 FRONTEND_ORIGIN
    if allowed_origin == "*" {
        log::warn!(
            "⚠️ FRONTEND_ORIGIN 未设置或为 '*',CORS 将放行所有 origin。生产环境必须明确设置!"
        );
    }

    // 简易 Prometheus 指标计数器(进程级)
    // 完整接入 metrics/prometheus 可后续再加
    // 注:trace_id 通过 request_meta::TraceId 提取器在每个 handler 中获取
    //    (用法:handler 加 `trace_id: TraceId` 参数即可)
    let server = HttpServer::new(move || {
        App::new()
            .state(Arc::clone(&app_state))
            .wrap(middleware::Logger::default())
            .wrap({
                let mut cors = Cors::new()
                    .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
                    .allowed_headers(vec![
                        "Authorization",
                        "Content-Type",
                        "X-Trace-Id",
                        "Idempotency-Key",
                    ])
                    .expose_headers(vec!["Authorization", "X-Trace-Id"])
                    .max_age(3600);

                // 开发环境允许所有 origin
                if allowed_origin == "*" {
                    cors = cors.send_wildcard();
                } else {
                    cors = cors
                        .allowed_origin(&allowed_origin)
                        .allowed_origin("https://servicewechat.com")
                        .supports_credentials();
                }

                cors.finish()
            })
            // 健康检查端点(Docker HEALTHCHECK / K8s probe 通用)
            .service(
                ntex::web::resource("/health")
                    .route(ntex::web::get().to(health_check)),
            )
            // Prometheus 指标端点(纯文本格式,够用)
            .service(
                ntex::web::resource("/metrics")
                    .route(ntex::web::get().to(metrics_endpoint)),
            )
            // API 路由配置
            .configure(|cfg| api::configure(cfg))
    })
    .workers(4)
    .bind("0.0.0.0:9831")?
    .run();

    // 运行 HTTP 服务器和 WebSocket 服务器
    tokio::select! {
        result = server => {
            if let Err(e) = result {
                log::error!("HTTP 服务器错误: {}", e);
            }
        }
        _ = ws_handle => {
            log::info!("WebSocket 服务器已停止");
        }
    };

    Ok(())
}

/// GET /health —— 简易健康检查
async fn health_check() -> &'static str {
    "ok"
}

/// GET /metrics —— Prometheus 文本格式的进程级指标
/// 当前暴露:进程启动时间 + 版本 + build info
/// 后续可接入 `metrics` + `metrics-exporter-prometheus` crate
async fn metrics_endpoint() -> ntex::web::HttpResponse {
    use std::time::{SystemTime, UNIX_EPOCH};
    let started_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format!(
        "# HELP may_store_info Build information\n\
         # TYPE may_store_info gauge\n\
         may_store_info{{version=\"{}\"}} 1\n\
         # HELP may_store_started_unix Process start time (Unix epoch)\n\
         # TYPE may_store_started_unix gauge\n\
         may_store_started_unix {}\n",
        env!("CARGO_PKG_VERSION"),
        started_unix
    );
    ntex::web::HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(body)
}
