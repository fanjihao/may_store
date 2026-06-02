use std::env;
use std::sync::Arc;

use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::cache::RedisCache;
use crate::errors::CustomError;

pub const TOKEN_SECRET_KEY: &[u8] = b"maystore";

/// 应用状态 - 包含所有共享资源
#[derive(Clone)]
pub struct AppState {
    pub db_pool: Pool<Postgres>,
    pub redis_cache: Arc<RedisCache>,
    pub jwt_secret: String,
    pub wx_app_id: String,
    pub wx_app_secret: String,
}

pub async fn init_app_state() -> Result<Arc<AppState>, CustomError> {
    let db_url = env::var("DATABASE_URL").expect("Please set DATABASE_URL");
    let redis_url = env::var("REDIS_URL").expect("Please set REDIS_URL");
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| "maystore_jwt_secret_key".to_string());
    let wx_app_id = env::var("WX_APP_ID").unwrap_or_default();
    let wx_app_secret = env::var("WX_APP_SECRET").unwrap_or_default();

    let redis_cache = match RedisCache::new(&redis_url) {
        Ok(cache) => Arc::new(cache),
        Err(err) => {
            eprintln!("Failed to connect to Redis: {}", err);
            return Err(CustomError::internal(format!("Redis连接失败: {err}")));
        }
    };

    let db_pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await?;

    let app_state = Arc::new(AppState {
        db_pool,
        redis_cache,
        jwt_secret,
        wx_app_id,
        wx_app_secret,
    });

    Ok(app_state)
}
