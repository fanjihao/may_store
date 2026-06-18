// API - 认证路由
// FSD.latest.md compliant - 微信登录

use ntex::web::{
    self,
    types::{Json, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::jwt;
use crate::utils::response::ApiResponse;

/// 配置认证路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/auth")
            .route("/wechat-login", web::post().to(wechat_login))
            .route("/refresh", web::post().to(refresh_token))
            .route("/logout", web::post().to(logout)),
    );
}

/// 刷新访问令牌请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenInput {
    pub refresh_token: String,
}

/// 刷新访问令牌响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

/// 刷新访问令牌
/// POST /api/auth/refresh
///
/// 使用 refresh_token 换取新的 access_token 和 refresh_token
/// refresh_token 一次性使用，用完立即失效并颁发新的 refresh_token
#[utoipa::path(
    post,
    path = "/api/auth/refresh",
    tag = "认证",
    request_body = RefreshTokenInput,
    responses(
        (status = 200, description = "刷新成功", body = RefreshTokenResponse),
        (status = 401, description = "refresh_token 无效或已过期"),
        (status = 403, description = "令牌已被撤销")
    ),
    security(())
)]
pub async fn refresh_token(
    state: State<Arc<AppState>>,
    input: Json<RefreshTokenInput>,
) -> Result<impl Responder, CustomError> {
    // 1. 校验 refresh token(签名 + 类型 + 黑名单 + 全设备撤销)
    let old = jwt::verify(
        &input.refresh_token,
        &state.jwt_secret,
        jwt::TokenType::Refresh,
        &state.redis_cache,
    )
    .await?;
    let user_id = old.user_id()?;

    // 2. 签发新对
    let (access_token, _) = jwt::issue_access(user_id, &state.jwt_secret)?;
    let (refresh_token, _) = jwt::issue_refresh(user_id, &state.jwt_secret)?;

    // 3. 旋转策略:旧 refresh 立即拉黑,防止重复使用(RFC 8725 §2.1)
    jwt::blacklist_jti(&old.jti, old.exp, &state.redis_cache).await?;

    Ok(ApiResponse::success(RefreshTokenResponse {
        access_token,
        refresh_token,
        expires_in: jwt::ACCESS_TTL_SECS,
    }))
}

/// 注销登录请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogoutInput {
    pub logout_all: Option<bool>,
}

/// 注销登录
/// POST /api/auth/logout
///
/// 将当前 access_token / refresh_token 加入 Redis 黑名单,
/// 黑名单 TTL 与 token 剩余有效期一致,过期后自动清理。
/// 后续 Auth 中间件会拒绝黑名单中的 token。
#[utoipa::path(
    post,
    path = "/api/auth/logout",
    tag = "认证",
    request_body = LogoutInput,
    responses(
        (status = 200, description = "注销成功"),
        (status = 401, description = "未登录")
    ),
    security(("bearer_auth" = []))
)]
pub async fn logout(
    token: UserToken,
    state: State<Arc<AppState>>,
    input: Json<LogoutInput>,
) -> Result<impl Responder, CustomError> {
    let redis = &state.redis_cache;

    // 1. 单 token 撤销:把当前 jti 加入黑名单
    jwt::blacklist_jti(&token.jti, token.exp, redis).await?;

    // 2. 可选:全设备撤销
    //    写入 user_revoked_at:{user_id} = now;此后所有 iat < now 的 token 都会被 jwt::verify 拒绝
    let logout_all = input.logout_all.unwrap_or(false);
    if logout_all {
        jwt::set_user_revoked_at(token.user_id, redis).await?;
    }

    // 3. 清除该用户的 user_public 缓存,强制下次重新加载
    let user_cache_key = format!("user:{}", token.user_id);
    if let Ok(mut conn) = redis.get_conn().await {
        use redis::AsyncCommands;
        let _: Result<(), _> = conn.del::<_, ()>(&user_cache_key).await;
    }

    log::info!(
        "user {} 已注销 (logout_all={}),jti={} 已加入黑名单",
        token.user_id,
        logout_all,
        token.jti
    );

    Ok(ApiResponse::success(serde_json::json!({
        "code": 0,
        "message": "success",
        "data": null
    })))
}

// `verify_token` 已删除;统一走 `crate::middlewares::jwt::verify`

/// 微信登录请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WechatLoginInput {
    pub code: String,                // 前端通过 wx.login() 获取的 code
    pub invite_code: Option<String>, // 可选的邀请码
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
    ),
    security(())
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
    let existing_user: Option<(i64, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT user_id, open_id, nick_name, avatar FROM users WHERE open_id = $1")
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

    // 生成 JWT token —— 统一走 jwt 模块
    let (access_token, _) = jwt::issue_access(user_id, &state.jwt_secret)?;
    let (refresh_token, _) = jwt::issue_refresh(user_id, &state.jwt_secret)?;

    // 更新最后登录时间
    sqlx::query("UPDATE users SET last_login_at = NOW() WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;

    Ok(ApiResponse::success(WechatLoginResponse {
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

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CustomError::InternalServerError(format!("微信响应解析失败: {}", e)))?;

    if let Some(errcode) = json.get("errcode").and_then(|v| v.as_i64()) {
        if errcode != 0 {
            let errmsg = json
                .get("errmsg")
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            return Err(CustomError::BadRequest(format!("微信认证失败: {}", errmsg)));
        }
    }

    let openid = json
        .get("openid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CustomError::BadRequest("微信响应缺少openid".into()))?;

    Ok(openid.to_string())
}

/// 创建微信用户
async fn create_wechat_user(db: &sqlx::PgPool, openid: &str) -> Result<i64, CustomError> {
    let user_id = idgenerator::IdInstance::next_id();
    // Generate a unique username for wechat users
    let username = format!("用户{:08x}", rand::random::<u32>());

    sqlx::query(
        // nick_name 默认 = username,避免新用户没有昵称导致前端展示空白
        // 用户后续可在 profile 修改 nick_name
        r#"INSERT INTO users (user_id, username, nick_name, open_id, status, role, login_method, created_at, updated_at)
           VALUES ($1, $2, $2, $3, 'ACTIVE', 'ORDERING'::user_role_enum, 'WEIXIN'::login_method_enum, NOW(), NOW())"#,
    )
    .bind(user_id)
    .bind(&username)
    .bind(openid)
    .execute(db)
    .await
    .map_err(|e| CustomError::InternalServerError(format!("创建用户失败: {}", e)))?;

    Ok(user_id)
}

// `generate_token` 与 `verify_token` 已删除;统一走 `crate::middlewares::jwt`
