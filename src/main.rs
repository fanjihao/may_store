mod config;
mod middlewares;

mod cache;
mod errors;
mod models;
mod openapi;
mod private;
mod routes;
mod utils;

mod game_im;
mod game_ws;

mod dashboard;
mod foods;
mod orders;
mod services; // 新增服务模块用于通知推送
mod upload;
mod users;
mod wishes; // 心愿与兑换模块
mod wx; // 看板与组活动

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

    // log
    init_logger();

    // 雪花id
    let options = IdGeneratorOptions::new().worker_id(1).worker_id_bit_len(6);
    let _ = IdInstance::init(options)?;

    // state
    let app_state = init_app_state().await?;
    let app_state_clone = Arc::clone(&app_state);

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

                // 开发环境允许所有 origin（FRONTEND_ORIGIN 未设置或为 *）
                if allowed_origin == "*" {
                    // ntex-cors 没有 allow_any_origin，用 send_wildcard 代替
                    cors = cors.send_wildcard();
                } else {
                    cors = cors
                        .allowed_origin(&allowed_origin)
                        .allowed_origin("https://servicewechat.com")
                        .supports_credentials();
                }

                cors.finish()
            })
            .configure(|cfg| routes::route(Arc::clone(&app_state), cfg))
    })
    .workers(4)
    .bind("0.0.0.0:9831")?
    .run();

    // 启动订单过期后台任务（不阻塞主服务器运行）
    let expiration_handle = tokio::spawn(orders::service::OrderService::run_expiration_worker(
        app_state_clone,
    ));

    // 运行 HTTP 服务器（阻塞直到停止）
    server.await?;

    // 等待后台任务结束（正常情况下不会返回，除非出现 panic 或关闭）
    let _ = expiration_handle.await;

    Ok(())
}
