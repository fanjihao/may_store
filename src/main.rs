mod cache;
mod errors;
mod models;
mod openapi;
mod routes;
mod utils;

mod wx_official;
mod users;
mod foods;
mod orders;
mod services; // 新增服务模块用于通知推送
mod wishes; // 心愿与兑换模块
mod dashboard; // 看板与组活动

use cache::RedisCache;
use dotenvy::dotenv;
use errors::CustomError;
use idgenerator::{IdGeneratorOptions, IdInstance};
use ntex::web::{middleware, App, HttpServer};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::{env, sync::Arc};

#[derive(Debug, Clone)]
pub struct AppState {
    pub db_pool: Pool<Postgres>,
    pub redis_cache: Arc<RedisCache>, // 添加Redis缓存
}

#[ntex::main]
async fn main() -> Result<(), CustomError> {
    dotenv().ok();

    // log
    env::set_var("RUST_LOG", "ntex=info");
    env_logger::init();

    // 雪花id
    let options = IdGeneratorOptions::new().worker_id(1).worker_id_bit_len(6);
    let _ = IdInstance::init(options)?;

    let db_url = env::var("DATABASE_URL").expect("Please set DATABASE_URL");
    let redis_url = env::var("REDIS_URL").expect("Please set REDIS_URL");

    // 初始化Redis缓存
    let redis_cache = match RedisCache::new(&redis_url) {
        Ok(cache) => Arc::new(cache),
        Err(err) => {
            eprintln!("Failed to connect to Redis: {}", err);
            return Err(CustomError::RedisError(err.to_string()));
        }
    };
    // state
    let app_state: Arc<AppState> = Arc::new(AppState {
        db_pool: PgPoolOptions::new()
            .max_connections(10)
            .connect(&db_url)
            .await?,
        redis_cache,
    });
    let app_state_clone = Arc::clone(&app_state);

    let server = HttpServer::new(move || {
        App::new()
            .state(Arc::clone(&app_state))
            .wrap(middleware::Logger::default())
            .configure(|cfg| routes::route(Arc::clone(&app_state), cfg))
    })
    .workers(4)
    .bind("0.0.0.0:9831")?
    .run();

    // 启动 HTTP 服务器
    let server_handle = tokio::spawn(server);

    // 定时任务：每分钟检查订单失效时间（30分钟未接单自动过期）
    let expiration_handle = tokio::spawn(orders::expiration::run_expiration_worker(app_state_clone));

    // 等待 HTTP 服务器和定时任务完成
    let _ = tokio::try_join!(server_handle, expiration_handle)?;

    Ok(())
}
