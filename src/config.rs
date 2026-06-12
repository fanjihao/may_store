use std::env;
use std::sync::Arc;

use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::cache::RedisCache;
use crate::errors::CustomError;

pub const TOKEN_SECRET_KEY: &[u8] = b"maystore";

/// 七牛云对象存储配置
/// 严格从环境变量读取，**严禁在代码库硬编码** AccessKey / SecretKey
#[derive(Clone, Debug)]
pub struct QiniuConfig {
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub region: QiniuRegion,
    pub upload_host: String,
    pub cdn_domain: String,
    pub token_expire_secs: u32,
}

/// 七牛存储区域
#[derive(Clone, Debug)]
pub enum QiniuRegion {
    /// 华东
    Z0,
    /// 华北
    Z1,
    /// 华南
    Z2,
    /// 北美
    Na0,
    /// 东南亚
    As0,
}

impl QiniuRegion {
    pub fn upload_host(&self) -> &'static str {
        match self {
            QiniuRegion::Z0 => "https://upload.qiniup.com",
            QiniuRegion::Z1 => "https://upload-z1.qiniup.com",
            QiniuRegion::Z2 => "https://upload-z2.qiniup.com",
            QiniuRegion::Na0 => "https://upload-na0.qiniup.com",
            QiniuRegion::As0 => "https://upload-as0.qiniup.com",
        }
    }
}

/// 应用状态 - 包含所有共享资源
#[derive(Clone)]
pub struct AppState {
    pub db_pool: Pool<Postgres>,
    pub redis_cache: Arc<RedisCache>,
    pub jwt_secret: String,
    pub wx_app_id: String,
    pub wx_app_secret: String,
    pub qiniu: Arc<QiniuConfig>,
}

pub async fn init_app_state() -> Result<Arc<AppState>, CustomError> {
    let db_url = env::var("DATABASE_URL").expect("Please set DATABASE_URL");
    let redis_url = env::var("REDIS_URL").expect("Please set REDIS_URL");
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| "maystore_jwt_secret_key".to_string());
    let wx_app_id = env::var("WX_APP_ID").unwrap_or_default();
    let wx_app_secret = env::var("WX_APP_SECRET").unwrap_or_default();

    // 七牛云配置
    let qiniu = Arc::new(QiniuConfig {
        access_key: env::var("QINIU_ACCESS_KEY").unwrap_or_default(),
        secret_key: env::var("QINIU_SECRET_KEY").unwrap_or_default(),
        bucket: env::var("QINIU_BUCKET").unwrap_or_else(|_| "may-store".to_string()),
        region: parse_qiniu_region(&env::var("QINIU_REGION").unwrap_or_else(|_| "z0".to_string())),
        upload_host: env::var("QINIU_UPLOAD_HOST").unwrap_or_default(),
        cdn_domain: env::var("QINIU_CDN_DOMAIN").unwrap_or_default(),
        token_expire_secs: env::var("QINIU_TOKEN_EXPIRE_S")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600),
    });

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
        qiniu,
    });

    Ok(app_state)
}

fn parse_qiniu_region(s: &str) -> QiniuRegion {
    match s.to_lowercase().as_str() {
        "z0" | "east" | "huadong" => QiniuRegion::Z0,
        "z1" | "north" | "huabei" => QiniuRegion::Z1,
        "z2" | "south" | "huanan" => QiniuRegion::Z2,
        "na0" | "north-america" => QiniuRegion::Na0,
        "as0" | "southeast-asia" => QiniuRegion::As0,
        _ => QiniuRegion::Z0,
    }
}
