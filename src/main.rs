// 主入口文件
// 遵循 FSD (Feature-Sliced Design) 架构

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
mod private;
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
    let ws_handle = tokio::spawn(async {
        use api::ws;
        if let Err(e) = ws::start_websocket_server("0.0.0.0:9832").await {
            log::error!("WebSocket 服务器错误: {}", e);
        }
    });

    let allowed_origin = env::var("FRONTEND_ORIGIN").unwrap_or_else(|_| "*".to_string());

    let server = HttpServer::new(move || {
        App::new()
            .state(Arc::clone(&app_state))
            .wrap(middleware::Logger::default())
            .wrap({
                let mut cors = Cors::new()
                    .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
                    .allowed_headers(vec!["Authorization", "Content-Type"])
                    .expose_headers(vec!["Authorization"])
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
