// API - 认证路由
// FSD.latest.md compliant - 微信登录

use ntex::web::{
    self, types::{Json, State}, HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;

/// 配置认证路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/auth")
            .route("/wechat-login", web::post().to(wechat_login)),
    );
}

/// 微信登录请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WechatLoginInput {
    pub code: String,       // 前端通过 wx.login() 获取的 code
    pub invite_code: Option<String>,  // 可选的邀请码
}

/// 微信登录响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WechatLoginResponse {
    pub user_id: i64,
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
    pub access_token: String,
    pub refresh_token: String,
    pub is_new_user: bool,
}

/// 微信登录
/// POST /api/auth/wechat-login
///
/// 流程:
/// 1. 前端调用 wx.login() 获取 code
/// 2. 前端请求本接口
/// 3. 后端使用 code 调用微信接口换取 openid
/// 4. 后端按 openid 查找用户
/// 5. 已存在用户返回访问令牌、刷新令牌和用户信息
/// 6. 不存在用户创建账号
#[utoipa::path(
    post,
    path = "/api/auth/wechat-login",
    tag = "认证",
    request_body = WechatLoginInput,
    responses(
        (status = 200, description = "登录成功", body = WechatLoginResponse),
        (status = 400, description = "参数错误"),
        (status = 401, description = "微信认证失败"),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn wechat_login(
    state: State<Arc<AppState>>,
    input: Json<WechatLoginInput>,
) -> Result<impl Responder, CustomError> {
    let code = &input.code;

    // 调用微信接口换取 openid 和 session_key
    let openid = get_wechat_openid(code, &state).await?;

    let db = &state.db_pool;

    // 查找已存在用户
    let existing_user: Option<(i64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT user_id, openid, nick_name, avatar FROM users WHERE openid = $1"
    )
    .bind(&openid)
    .fetch_optional(db)
    .await?;

    let is_new_user = existing_user.is_none();
    let (user_id, nickname, avatar_url) = if let Some((uid, _, nick, ava)) = existing_user {
        (uid, nick, ava)
    } else {
        // 创建新用户
        let user_id = create_wechat_user(db, &openid).await?;
        (user_id, None, None)
    };

    // 生成 JWT token
    let access_token = generate_token(user_id, "access", &state.jwt_secret)?;
    let refresh_token = generate_token(user_id, "refresh", &state.jwt_secret)?;

    // 更新最后登录时间
    sqlx::query("UPDATE users SET last_login_at = NOW() WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;

    Ok(HttpResponse::Ok().json(&WechatLoginResponse {
        user_id,
        nickname,
        avatar_url,
        access_token,
        refresh_token,
        is_new_user,
    }))
}

/// 调用微信接口获取 openid
async fn get_wechat_openid(code: &str, state: &AppState) -> Result<String, CustomError> {
    let appid = &state.wx_app_id;
    let secret = &state.wx_app_secret;

    let url = format!(
        "https://api.weixin.qq.com/sns/jscode2session?appid={}&secret={}&js_code={}&grant_type=authorization_code",
        appid, secret, code
    );

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| CustomError::InternalServerError(format!("微信请求失败: {}", e)))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| CustomError::InternalServerError(format!("微信响应解析失败: {}", e)))?;

    if let Some(errcode) = json.get("errcode").and_then(|v| v.as_i64()) {
        if errcode != 0 {
            let errmsg = json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("未知错误");
            return Err(CustomError::Unauthorized(format!("微信认证失败: {}", errmsg)));
        }
    }

    let openid = json.get("openid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CustomError::BadRequest("微信响应缺少openid".into()))?;

    Ok(openid.to_string())
}

/// 创建微信用户
async fn create_wechat_user(db: &sqlx::PgPool, openid: &str) -> Result<i64, CustomError> {
    let user_id = idgenerator::IdInstance::next_id();

    sqlx::query(
        r#"INSERT INTO users (user_id, openid, status, created_at, updated_at)
           VALUES ($1, $2, 'ACTIVE', NOW(), NOW())"#
    )
    .bind(user_id)
    .bind(openid)
    .execute(db)
    .await
    .map_err(|e| CustomError::InternalServerError(format!("创建用户失败: {}", e)))?;

    Ok(user_id)
}

/// 生成JWT token
fn generate_token(user_id: i64, token_type: &str, secret: &str) -> Result<String, CustomError> {
    use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
    use chrono::{Utc, Duration};

    let expiration = if token_type == "access" {
        Utc::now() + Duration::hours(2)
    } else {
        Utc::now() + Duration::days(30)
    };

    let claims = serde_json::json!({
        "sub": user_id,
        "type": token_type,
        "exp": expiration.timestamp(),
        "iat": Utc::now().timestamp()
    });

    let header = Header::default();
    let key = EncodingKey::from_secret(secret.as_bytes());

    encode(&header, &claims, &key)
        .map_err(|e| CustomError::InternalServerError(format!("Token生成失败: {}", e)))
}