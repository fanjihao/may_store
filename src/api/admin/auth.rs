// API 层 - 后台管理员登录
// POST /api/admin/auth/login: bcrypt 验密码 + 幂等建 admin 用户 + 签 JWT

use ntex::web::{
    self,
    types::{Json, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::jwt::{self, TokenType};
use crate::utils::response::ApiResponse;

const ADMIN_USERNAME: &str = "admin";
const ADMIN_FIXED_USER_ID: i64 = 1;
const ADMIN_DEFAULT_ROLE: &str = "SUPER_ADMIN";

/// 登录请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLoginInput {
    pub password: String,
}

/// 登录响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminLoginResponse {
    pub access_token: String,
    pub expires_at: i64,    // Unix 秒
    pub role: String,       // "SUPER_ADMIN"
}

/// 配置后台管理 auth 路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/admin/auth")
            .route("/login", web::post().to(admin_login)),
    );
}

/// POST /api/admin/auth/login
#[utoipa::path(
    post,
    path = "/api/admin/auth/login",
    tag = "后台管理认证",
    request_body = AdminLoginInput,
    responses(
        (status = 200, description = "登录成功", body = AdminLoginResponse),
        (status = 401, description = "密码错误"),
        (status = 500, description = "服务端未配置 ADMIN_PASSWORD_HASH")
    )
)]
pub async fn admin_login(
    state: State<Arc<AppState>>,
    body: Json<AdminLoginInput>,
) -> Result<impl Responder, CustomError> {
    let password_hash = std::env::var("ADMIN_PASSWORD_HASH")
        .map_err(|_| CustomError::internal(String::from("服务端未配置 ADMIN_PASSWORD_HASH")))?;

    // 1. bcrypt 验密码
    let valid = bcrypt::verify(body.password.as_bytes(), &password_hash)
        .map_err(|e| CustomError::internal(format!("bcrypt 验证失败: {}", e)))?;
    if !valid {
        return Err(CustomError::auth_invalid_token("管理员密码错误"));
    }

    let db = &state.db_pool;

    // 2. 幂等建 users 行 (id=1, username='admin')
    // 注意:users.user_id 是 BIGSERIAL,显式 insert id=1 需 ON CONFLICT
    sqlx::query(
        r#"INSERT INTO users (user_id, username) VALUES ($1, $2)
           ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(ADMIN_FIXED_USER_ID)
    .bind(ADMIN_USERNAME)
    .execute(db)
    .await?;

    // 3. 幂等建 admin_users 行
    sqlx::query(
        r#"INSERT INTO admin_users (user_id, role, status) VALUES ($1, $2::admin_role_enum, 'ACTIVE')
           ON CONFLICT (user_id) DO UPDATE SET status = 'ACTIVE'"#,
    )
    .bind(ADMIN_FIXED_USER_ID)
    .bind(ADMIN_DEFAULT_ROLE)
    .execute(db)
    .await?;

    // 4. 读 role + admin_id 出来
    let row: (i64, String) = sqlx::query_as(
        "SELECT admin_id, role::text FROM admin_users WHERE user_id = $1",
    )
    .bind(ADMIN_FIXED_USER_ID)
    .fetch_one(db)
    .await?;

    // 5. 签 JWT
    let (token, claims) = jwt::issue_access(ADMIN_FIXED_USER_ID, &state.jwt_secret)
        .map_err(|e| CustomError::internal(format!("JWT 签发失败: {}", e)))?;

    Ok(ApiResponse::success(AdminLoginResponse {
        access_token: token,
        expires_at: claims.exp,
        role: row.1,
    }))
}

#[cfg(test)]
mod tests {
    // 占位：bcrypt 验签和 DB 写入是端到端的，集成测试基建到位再加
    // 见 plan 文档 §6.2 "不写集成测试"
    #[test]
    fn admin_login_input_deserializes_password() {
        // 单元测试 1: AdminLoginInput 反序列化
        let json = r#"{"password": "secret"}"#;
        let parsed: super::AdminLoginInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.password, "secret");
    }

    #[test]
    fn admin_login_response_serializes_camel_case() {
        // 单元测试 2: AdminLoginResponse 序列化为 camelCase
        let r = super::AdminLoginResponse {
            access_token: "tok".to_string(),
            expires_at: 1234567890,
            role: "SUPER_ADMIN".to_string(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("accessToken"));
        assert!(json.contains("expiresAt"));
    }
}