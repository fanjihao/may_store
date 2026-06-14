//! 统一的 JWT 编码 / 解码 / 撤销层
//!
//! 全项目唯一的 token 签发与校验入口。所有 HTTP / WS / refresh / logout
//! 都必须经过这里,任何重复实现都属于 bug。
//!
//! 规范遵循:
//! - RFC 7519 (JSON Web Token):标准 claim 字段 `sub`/`jti`/`iat`/`nbf`/`exp`/`iss`
//! - RFC 8725 (JWT BCP):算法锁定 HS256;`exp`/`iss` 强制校验;refresh 旋转;
//!   单 token 撤销 + 用户级全撤销分离
//!
//! Claim 形状(单一权威):
//! ```text
//! { sub, jti, iat, nbf, exp, iss, typ }
//! ```
//! `sub` 为字符串(RFC 7519 §4.1.2),`typ` 为私有 claim ("access"|"refresh")。
//!
//! 撤销机制:
//! - 单 token 撤销:Redis key `token_blacklist:{jti}`,TTL = token 剩余有效期
//! - 用户级全撤销:Redis key `user_revoked_at:{user_id}`,值=Unix 时间戳;
//!   校验时 `claims.iat < revoked_at` 则拒绝(注销时刻之前签发的全部失效)

use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cache::RedisCache;
use crate::errors::CustomError;

/// JWT issuer,固定字符串。多服务环境下用于区分签发方。
pub const ISSUER: &str = "may_store";

/// Access token 寿命:2 小时(RFC 8725 §2.7 推荐 ≤1h,这里折中)
pub const ACCESS_TTL_SECS: i64 = 2 * 60 * 60;

/// Refresh token 寿命:30 天
pub const REFRESH_TTL_SECS: i64 = 30 * 24 * 60 * 60;

/// 私有 claim `typ`:区分 access 与 refresh,防止互换使用
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Access,
    Refresh,
}

/// 单一权威 JWT claim 结构。新代码请勿再定义平行的 claim 结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject —— user_id 的字符串形式(RFC 7519 §4.1.2 要求 StringOrURI)
    pub sub: String,
    /// JWT ID —— UUID v4,黑名单/审计的唯一标识
    pub jti: String,
    /// Issued At —— Unix 秒
    pub iat: i64,
    /// Not Before —— 此处等于 `iat`,无时钟容差需求
    pub nbf: i64,
    /// Expiry —— Unix 秒
    pub exp: i64,
    /// Issuer —— 固定为 `ISSUER`
    pub iss: String,
    /// Token 类型:access / refresh
    pub typ: TokenType,
}

impl Claims {
    /// 把 `sub` 解析回 i64 user_id。失败返回 `auth_invalid_token`。
    pub fn user_id(&self) -> Result<i64, CustomError> {
        self.sub
            .parse::<i64>()
            .map_err(|_| CustomError::auth_invalid_token("token sub 非合法整数"))
    }
}

/// 内部签发函数,统一 `Header::new(HS256)` + 标准 claim 填充
fn issue(
    user_id: i64,
    secret: &str,
    typ: TokenType,
    ttl_secs: i64,
) -> Result<(String, Claims), CustomError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        jti: Uuid::new_v4().to_string(),
        iat: now,
        nbf: now,
        exp: now + ttl_secs,
        iss: ISSUER.to_string(),
        typ,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| CustomError::internal(format!("token encode 失败: {}", e)))?;
    Ok((token, claims))
}

/// 签发 access token
pub fn issue_access(user_id: i64, secret: &str) -> Result<(String, Claims), CustomError> {
    issue(user_id, secret, TokenType::Access, ACCESS_TTL_SECS)
}

/// 签发 refresh token
pub fn issue_refresh(user_id: i64, secret: &str) -> Result<(String, Claims), CustomError> {
    issue(user_id, secret, TokenType::Refresh, REFRESH_TTL_SECS)
}

/// 一站式校验:签名/算法/exp/iss/typ + jti 黑名单 + 用户级全撤销
///
/// 任一环节失败立即返回错误。Redis 故障 fail-closed(返回 internal error)。
pub async fn verify(
    token: &str,
    secret: &str,
    expected: TokenType,
    redis: &RedisCache,
) -> Result<Claims, CustomError> {
    // 1. 签名 + 算法 + exp + iss 校验(jsonwebtoken 内置)
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "sub", "iss"]);

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| CustomError::auth_invalid_token(format!("token 解码失败: {}", e)))?;
    let claims = data.claims;

    // 2. 类型校验
    if claims.typ != expected {
        return Err(CustomError::auth_invalid_token(format!(
            "token 类型不匹配:期望 {:?},实际 {:?}",
            expected, claims.typ
        )));
    }

    // 3. Redis 撤销检查 —— get_conn 失败一律 fail-closed
    let mut conn = redis
        .get_conn()
        .await
        .map_err(|e| CustomError::internal(format!("Redis 不可用: {}", e)))?;

    // 3a. jti 单点黑名单(logout 写入)
    let blacklist_key = format!("token_blacklist:{}", claims.jti);
    let blacklisted: Option<String> = conn
        .get(&blacklist_key)
        .await
        .map_err(|e| CustomError::internal(format!("Redis 黑名单查询失败: {}", e)))?;
    if blacklisted.is_some() {
        return Err(CustomError::auth_token_revoked("token 已撤销"));
    }

    // 3b. 用户级全撤销(logout_all 写入)
    let revoked_key = format!("user_revoked_at:{}", claims.sub);
    let revoked_at: Option<i64> = conn
        .get(&revoked_key)
        .await
        .map_err(|e| CustomError::internal(format!("Redis 全撤销查询失败: {}", e)))?;
    if let Some(t) = revoked_at {
        if claims.iat < t {
            return Err(CustomError::auth_token_revoked("用户已全设备注销"));
        }
    }

    Ok(claims)
}

/// 把 jti 加入黑名单。TTL 取 `exp - now`,最小 1 秒。
///
/// 调用前提:已经从 token 中拿到了 `jti` 和 `exp`(通常是刚 verify 完)。
pub async fn blacklist_jti(jti: &str, exp: i64, redis: &RedisCache) -> Result<(), CustomError> {
    let now = Utc::now().timestamp();
    let ttl = (exp - now).max(1) as usize;
    let mut conn = redis
        .get_conn()
        .await
        .map_err(|e| CustomError::internal(format!("Redis 不可用: {}", e)))?;
    let key = format!("token_blacklist:{}", jti);
    conn.set_ex::<_, _, ()>(&key, "1", ttl)
        .await
        .map_err(|e| CustomError::internal(format!("写入黑名单失败: {}", e)))?;
    Ok(())
}

/// 用户级全撤销:写入当前时间戳到 `user_revoked_at:{user_id}`,
/// TTL = REFRESH_TTL_SECS(覆盖任何仍可能存在的有效 token)。
///
/// 此后所有 `iat < 写入时刻` 的 token 都会在 [`verify`] 中被拒绝。
pub async fn set_user_revoked_at(user_id: i64, redis: &RedisCache) -> Result<(), CustomError> {
    let now = Utc::now().timestamp();
    let mut conn = redis
        .get_conn()
        .await
        .map_err(|e| CustomError::internal(format!("Redis 不可用: {}", e)))?;
    let key = format!("user_revoked_at:{}", user_id);
    conn.set_ex::<_, _, ()>(&key, now, REFRESH_TTL_SECS as usize)
        .await
        .map_err(|e| CustomError::internal(format!("写入全设备撤销失败: {}", e)))?;
    Ok(())
}
