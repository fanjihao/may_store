// API 层 - 用户模块
// FSD.latest.md compliant - 用户基础信息

use ntex::web::{self, ServiceConfig};
use std::sync::Arc;
use sqlx::Row;

use crate::config::AppState;

/// 配置用户相关路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/users/me")
            .route("", web::get().to(get_current_info))
            .route("", web::patch().to(update_info))
            .route("/groups", web::get().to(get_user_groups))
            .route("/delete", web::post().to(delete_account)),
    );
}

// ========== 用户 Handler 函数 ==========

use ntex::web::{
    types::{Json, Query, State},
    Responder,
};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    application::user_service::UserService,
    domain::user::{
        IsRegisterQuery, IsRegisterResponse, LoginInput, LoginResponse, ProfileUpdateInput,
        RegisterInput, UserInfoResponse, UserPublic,
    },
    errors::CustomError,
    middlewares::auth::UserToken,
    utils::response::ApiResponse,
};

// ========== FSD v2 用户中心接口 ==========

/// 获取当前用户信息
/// GET /api/users/me
#[utoipa::path(
    get,
    path = "/api/users/me",
    operation_id = "get_current_info",
    tag = "用户",
    summary = "获取当前登录用户信息",
    responses(
        (status = 200, description = "获取成功", body = UserPublic),
        (status = 401, body = CustomError)
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_current_info(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let res = UserService::get_current_info(token.user_id, &state).await?;
    Ok(ApiResponse::success(res))
}

/// 更新用户资料
/// PATCH /api/users/me
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfoInput {
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
    pub phone: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/users/me",
    operation_id = "update_info",
    tag = "用户",
    summary = "更新当前用户资料",
    request_body = UpdateInfoInput,
    responses(
        (status = 200, description = "更新成功", body = UserPublic),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_info(
    token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<UpdateInfoInput>,
) -> Result<impl Responder, CustomError> {
    let input = ProfileUpdateInput {
        username: "".to_string(), // not used for update
        nick_name: data.nickname.clone(),
        avatar: data.avatar_url.clone(),
        phone: data.phone.clone(),
        gender: None,
        birthday: None,
        new_password: None,
        old_password: None,
        new_username: None,
    };
    let res = UserService::change_info(token.user_id, input, &state).await?;
    Ok(ApiResponse::success(res))
}

/// 获取用户的组列表
/// GET /api/users/me/groups
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserGroupItem {
    pub group_id: i64,
    pub group_name: String,
    pub my_role: String,
    pub member_count: i32,
    pub diamond_balance: i64,
    pub level: i32,
    pub joined_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserGroupsResponse {
    pub groups: Vec<UserGroupItem>,
}

#[utoipa::path(
    get,
    path = "/api/users/me/groups",
    operation_id = "get_user_groups",
    tag = "用户",
    summary = "获取当前用户的组列表",
    responses(
        (status = 200, description = "获取成功", body = UserGroupsResponse),
        (status = 401, body = CustomError)
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_user_groups(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    let rows = sqlx::query(
        r#"SELECT g.group_id, g.group_name,
           CASE WHEN g.buyer_user_id = $1 THEN 'BUYER' ELSE 'SELLER' END as my_role,
           (SELECT COUNT(*) FROM association_group_members WHERE group_id = g.group_id) as member_count,
           g.diamond, g.level, g.created_at
           FROM association_groups g
           JOIN association_group_members gm ON gm.group_id = g.group_id
           WHERE gm.user_id = $1 AND gm.is_primary = true
           ORDER BY g.created_at DESC"#,
    )
    .bind(token.user_id)
    .fetch_all(db)
    .await?;

    let groups: Vec<UserGroupItem> = rows
        .into_iter()
        .map(|r| UserGroupItem {
            group_id: r.get::<i64, _>("group_id"),
            group_name: r.get::<String, _>("group_name"),
            my_role: r.get::<String, _>("my_role"),
            member_count: r.get::<i64, _>("member_count") as i32,
            diamond_balance: r.get::<i32, _>("diamond") as i64,
            level: r.get::<i32, _>("level"),
            joined_at: r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        })
        .collect();

    Ok(ApiResponse::success(UserGroupsResponse { groups }))
}

/// 账号注销
/// POST /api/users/me/delete
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountInput {
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountResponse {
    pub deleted_at: String,
    pub recovery_deadline: String,
}

#[utoipa::path(
    post,
    path = "/api/users/me/delete",
    operation_id = "delete_account",
    tag = "用户",
    summary = "注销当前账号",
    request_body = DeleteAccountInput,
    responses(
        (status = 200, description = "注销成功", body = DeleteAccountResponse),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_account(
    token: UserToken,
    state: State<Arc<AppState>>,
    data: Json<DeleteAccountInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 保留 data 引用,等 users 表加上 delete_reason 列后再启用。
    let _ = &data;

    // 软删除用户
    // 注意:users 表当前没有 deleted_at / delete_reason 列,仅依靠 status='DELETED' 标记。
    // 如需保留删除时间和原因用于恢复窗口,需要 ALTER TABLE 加列。
    let deleted_at = chrono::Utc::now();
    let recovery_deadline = deleted_at + chrono::Duration::days(30);

    sqlx::query(
        r#"UPDATE users SET status = 'DELETED'
           WHERE user_id = $1"#,
    )
    .bind(token.user_id)
    .execute(db)
    .await?;

    Ok(ApiResponse::success(DeleteAccountResponse {
        deleted_at: deleted_at.to_rfc3339(),
        recovery_deadline: recovery_deadline.to_rfc3339(),
    }))
}

// ========== 遗留接口已移除 ==========
//
// `register` / `login` / `get_user_info` / `is_register` 这 4 个 handler
// 在 `configure()` 中从未注册,#[utoipa::path] 也未挂到 openapi.rs,
// 是 v3 重构后遗留的死代码。所有账号入口统一走 /api/auth/wechat-login
// (业务方决定不再支持密码登录)。
//
// 如需恢复:重新挂载到 `cfg.service(scope("/api/auth")...)` 并补 utoipa 路径。
