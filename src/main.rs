mod errors;
mod utils;
mod models;
mod upload;
mod users;
mod foods;
mod orders;
mod wishes;
mod dashboard;
mod wx_official;
mod openapi;
mod routes;

use dotenvy::dotenv;
use errors::CustomError;
use idgenerator::{IdGeneratorOptions, IdInstance};
use ntex::web::{middleware, App, HttpServer};
use orders::update::check_order_expiration;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::{env, sync::Arc};

#[derive(Debug, Clone)]
pub struct AppState {
    pub db_pool: Pool<Postgres>,
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

    // state
    let app_state: Arc<AppState> = Arc::new(AppState {
        db_pool: PgPoolOptions::new()
            .max_connections(10)
            .connect(&db_url)
            .await?,
    });
    let app_state_clone = Arc::clone(&app_state);

    let server = HttpServer::new(move || {
        App::new()
            .state(Arc::clone(&app_state))
            .wrap(middleware::Logger::default())
            .configure(|cfg| routes::route(Arc::clone(&app_state), cfg))
    })
    .workers(4)
    .bind("0.0.0.0:9800")?
    .run();

    // 启动 HTTP 服务器
    let server_handle = tokio::spawn(server);

    // 定时任务：每分钟检查订单失效时间
    let task = tokio::spawn(check_order_expiration(app_state_clone));

    // 等待 HTTP 服务器和定时任务完成
    let _ = tokio::try_join!(server_handle, task)?;

    Ok(())
}

