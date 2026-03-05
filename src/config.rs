use std::env;
use std::sync::Arc;

use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::cache::RedisCache;
use crate::errors::CustomError;
use crate::game_im::models::ImConfig;
use crate::game_ws;

pub const TOKEN_SECRET_KEY: &[u8] = b"maystore";

#[derive(Debug, Clone)]
pub struct AppState {
    pub db_pool: Pool<Postgres>,
    pub redis_cache: Arc<RedisCache>,
    pub im_config: Option<Arc<ImConfig>>,
    pub game_hub: Arc<game_ws::service::GameHub>,
}

pub async fn init_app_state() -> Result<Arc<AppState>, CustomError> {
    let db_url = env::var("DATABASE_URL").expect("Please set DATABASE_URL");
    let redis_url = env::var("REDIS_URL").expect("Please set REDIS_URL");

    let im_config = match ImConfig::from_env() {
        Ok(v) => Some(Arc::new(v)),
        Err(_) => None,
    };

    let redis_cache = match RedisCache::new(&redis_url) {
        Ok(cache) => Arc::new(cache),
        Err(err) => {
            eprintln!("Failed to connect to Redis: {}", err);
            return Err(CustomError::internal(format!("Redis连接失败: {err}")));
        }
    };

    let app_state = Arc::new(AppState {
        db_pool: PgPoolOptions::new()
            .max_connections(10)
            .connect(&db_url)
            .await?,
        redis_cache,
        im_config,
        game_hub: Arc::new(game_ws::service::GameHub::new()),
    });

    Ok(app_state)
}
