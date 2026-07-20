use std::env;
use std::sync::Arc;

use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::cache::RedisCache;
use crate::errors::CustomError;

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

/// 启动时读取必须的环境变量,缺失立即 panic 防止弱默认值
fn require_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("必须设置环境变量: {name}"))
}

/// 启动时读取可选环境变量,缺失返回空串(调用方自行判断启用/禁用)
fn optional_env(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

pub async fn init_app_state() -> Result<Arc<AppState>, CustomError> {
    // 必填项:DB / Redis / JWT 强校验,绝不接受默认值
    let db_url = require_env("DATABASE_URL");
    let redis_url = require_env("REDIS_URL");
    let jwt_secret = require_env("JWT_SECRET");

    // 校验 JWT 强度(至少 32 字节随机),防止弱密钥
    if jwt_secret.len() < 32 {
        panic!(
            "JWT_SECRET 长度不足 32 字符(当前 {}),请使用 openssl rand -base64 32 生成",
            jwt_secret.len()
        );
    }

    // 可选项:微信 / Qiniu / Tencent IM,空值表示对应功能未启用
    let wx_app_id = optional_env("WX_APP_ID");
    let wx_app_secret = optional_env("WX_APP_SECRET");

    // 七牛云配置
    let qiniu = Arc::new(QiniuConfig {
        access_key: optional_env("QINIU_ACCESS_KEY"),
        secret_key: optional_env("QINIU_SECRET_KEY"),
        bucket: env::var("QINIU_BUCKET").unwrap_or_else(|_| "may-store".to_string()),
        region: parse_qiniu_region(&env::var("QINIU_REGION").unwrap_or_else(|_| "z0".to_string())),
        upload_host: optional_env("QINIU_UPLOAD_HOST"),
        cdn_domain: optional_env("QINIU_CDN_DOMAIN"),
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

    // 生产级连接池配置
    let db_pool = PgPoolOptions::new()
        .max_connections(
            env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(20),
        )
        .min_connections(
            env::var("DB_MIN_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
        )
        .acquire_timeout(std::time::Duration::from_secs(
            env::var("DB_ACQUIRE_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
        ))
        .idle_timeout(std::time::Duration::from_secs(
            env::var("DB_IDLE_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(600),
        ))
        .max_lifetime(std::time::Duration::from_secs(
            env::var("DB_MAX_LIFETIME_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1800),
        ))
        .connect(&db_url)
        .await?;

    // v3.sql 用于空库初始化；此处只执行 forward-only 增量迁移。
    // SQLx migrator 使用数据库锁，多个实例同时启动时不会重复应用同一版本。
    sqlx::migrate!("./migrations")
        .run(&db_pool)
        .await
        .map_err(|e| CustomError::internal(format!("数据库迁移失败: {e}")))?;

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
