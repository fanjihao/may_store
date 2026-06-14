//! 测试用 JWT 生成工具
//!
//! 用法:
//! ```bash
//! cargo run --example issue_token                    # access token for user_id=1
//! cargo run --example issue_token -- 12345           # access token for user_id=12345
//! cargo run --example issue_token -- 12345 refresh   # refresh token
//! ```
//!
//! 输出:stderr 打印 claim 摘要,stdout 只打印 token 字符串(便于 pipe 到 curl)
//!
//! Claim 结构与 `src/middlewares/jwt.rs::Claims` 保持一致。改一边记得同步。

use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

const ISSUER: &str = "may_store";
const ACCESS_TTL_SECS: i64 = 2 * 60 * 60;
const REFRESH_TTL_SECS: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TokenType {
    Access,
    Refresh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    jti: String,
    iat: i64,
    nbf: i64,
    exp: i64,
    iss: String,
    typ: TokenType,
}

fn main() {
    dotenvy::dotenv().ok();

    let secret = env::var("JWT_SECRET").expect("JWT_SECRET 必须在 .env 中设置");
    if secret.len() < 32 {
        panic!(
            "JWT_SECRET 长度不足 32 (实际 {}),与 config::init_app_state 的强度校验对齐",
            secret.len()
        );
    }

    let mut args = env::args().skip(1);
    let user_id: i64 = args
        .next()
        .as_deref()
        .unwrap_or("1")
        .parse()
        .expect("user_id 必须是整数");
    let typ = match args.next().as_deref().unwrap_or("access") {
        "access" => TokenType::Access,
        "refresh" => TokenType::Refresh,
        other => panic!("token type 必须是 access / refresh,收到: {}", other),
    };

    let ttl = match typ {
        TokenType::Access => ACCESS_TTL_SECS,
        TokenType::Refresh => REFRESH_TTL_SECS,
    };

    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        jti: Uuid::new_v4().to_string(),
        iat: now,
        nbf: now,
        exp: now + ttl,
        iss: ISSUER.to_string(),
        typ,
    };

    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("token encode 失败");

    eprintln!("──── JWT generated ────");
    eprintln!("  sub (user_id): {}", claims.sub);
    eprintln!("  jti:           {}", claims.jti);
    eprintln!("  typ:           {:?}", claims.typ);
    eprintln!("  iss:           {}", claims.iss);
    eprintln!("  iat:           {}", claims.iat);
    eprintln!("  nbf:           {}", claims.nbf);
    eprintln!("  exp:           {}  (in {}s)", claims.exp, ttl);
    eprintln!();
    eprintln!("Authorization: Bearer <below>");
    eprintln!();
    println!("{}", token);
}
