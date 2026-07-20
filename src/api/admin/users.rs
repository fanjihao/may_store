// API 层 - 后台用户管理

use ntex::web::{
    self,
    types::{Json, Path, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::jwt;
use crate::utils::response::ApiResponse;

/// user_role_enum 合法值(查 v3.sql L332)
const VALID_ROLES: &[&str] = &["ORDERING", "RECEIVING", "ADMIN"];

/// user_status_enum 合法值(查 v3.sql L327)
const VALID_STATUSES: &[&str] = &["ACTIVE", "BANNED", "DELETED"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostCommitStatusAction {
    None,
    ClearCache,
    RevokeAndClearCache,
}

fn post_commit_status_action(old_status: &str, new_status: &str) -> PostCommitStatusAction {
    if old_status == "ACTIVE" && matches!(new_status, "BANNED" | "DELETED") {
        PostCommitStatusAction::RevokeAndClearCache
    } else if old_status != new_status && new_status == "ACTIVE" {
        PostCommitStatusAction::ClearCache
    } else {
        PostCommitStatusAction::None
    }
}

/// 用户名最大长度(查 users.username VARCHAR(64))
const USERNAME_MAX_LEN: usize = 64;

/// 昵称最大长度(查 users.nick_name VARCHAR(64))
const NICK_NAME_MAX_LEN: usize = 64;

/// 用户更新输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserUpdateInput {
    pub username: Option<String>,
    pub nick_name: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
}

/// 用户输出(响应体)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserOut {
    pub user_id: i64,
    pub username: String,
    pub nick_name: Option<String>,
    pub role: String,
    pub status: String,
    pub love_point: i32,
    pub diamond: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 校验用户更新输入
pub fn validate_user_update(input: &UserUpdateInput) -> Result<(), CustomError> {
    if input.username.is_none()
        && input.nick_name.is_none()
        && input.role.is_none()
        && input.status.is_none()
    {
        return Err(CustomError::BadRequest("至少需要更新一个字段".into()));
    }

    if let Some(ref u) = input.username {
        if u.is_empty() {
            return Err(CustomError::BadRequest("username 不能为空".into()));
        }
        if u.len() > USERNAME_MAX_LEN {
            return Err(CustomError::BadRequest(format!(
                "username 长度不能超过 {}",
                USERNAME_MAX_LEN
            )));
        }
    }

    if let Some(ref n) = input.nick_name {
        if n.is_empty() {
            return Err(CustomError::BadRequest("nickName 不能为空".into()));
        }
        if n.len() > NICK_NAME_MAX_LEN {
            return Err(CustomError::BadRequest(format!(
                "nickName 长度不能超过 {}",
                NICK_NAME_MAX_LEN
            )));
        }
    }

    if let Some(ref r) = input.role {
        if !VALID_ROLES.contains(&r.as_str()) {
            return Err(CustomError::BadRequest(format!(
                "role 必须是 {:?} 之一",
                VALID_ROLES
            )));
        }
    }

    if let Some(ref s) = input.status {
        if !VALID_STATUSES.contains(&s.as_str()) {
            return Err(CustomError::BadRequest(format!(
                "status 必须是 {:?} 之一",
                VALID_STATUSES
            )));
        }
    }

    Ok(())
}

/// 配置路由(在 admin::routes::configure 里被调)
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(web::scope("/api/admin/users").route("/{user_id}", web::patch().to(update_user)));
}

/// PATCH /api/admin/users/{user_id}
///
/// 管理员修改用户字段。至少一个字段。改 username 需唯一性校验(409)。
/// 每次 PATCH 写 audit_logs(operator_id=admin, action_type='USER_UPDATE', detail 含原值/新值)
#[utoipa::path(
    patch,
    path = "/api/admin/users/{user_id}",
    tag = "后台管理 - 用户",
    params(("user_id" = i64, Path, description = "用户 ID")),
    request_body = UserUpdateInput,
    responses(
        (status = 200, description = "更新成功", body = UserOut),
        (status = 400, description = "字段非法"),
        (status = 401, description = "未登录"),
        (status = 404, description = "用户不存在"),
        (status = 409, description = "username 已被占用")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_user(
    state: State<Arc<AppState>>,
    admin: crate::middlewares::admin_auth::AdminToken,
    path: Path<i64>,
    body: Json<UserUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let user_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    validate_user_update(&input)?;

    let mut tx = db.begin().await?;

    let row: Option<(String, Option<String>, String, String)> = sqlx::query_as(
        r#"SELECT username, nick_name, role::text, status::text
           FROM users WHERE user_id = $1 FOR UPDATE"#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (old_username, old_nick_name, old_role, old_status) = match row {
        Some(r) => r,
        None => return Err(CustomError::NotFound("用户不存在".into())),
    };

    if let Some(ref new_username) = input.username {
        if new_username != &old_username {
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT user_id FROM users WHERE username = $1 AND user_id != $2 LIMIT 1",
            )
            .bind(new_username)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
            if exists.is_some() {
                return Err(CustomError::Conflict("该用户名已被使用".into()));
            }
        }
    }

    let new_username = input
        .username
        .clone()
        .unwrap_or_else(|| old_username.clone());
    let new_nick_name = input.nick_name.clone().or_else(|| old_nick_name.clone());
    let new_role = input.role.clone().unwrap_or_else(|| old_role.clone());
    let new_status = input.status.clone().unwrap_or_else(|| old_status.clone());
    let status_action = post_commit_status_action(&old_status, &new_status);

    sqlx::query(
        r#"UPDATE users
           SET username = $1, nick_name = $2, role = $3::user_role_enum, status = $4::user_status_enum, updated_at = NOW()
           WHERE user_id = $5"#,
    )
    .bind(&new_username)
    .bind(&new_nick_name)
    .bind(&new_role)
    .bind(&new_status)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    let detail = serde_json::json!({
        "user_id": user_id,
        "before": {
            "username": old_username, "nick_name": old_nick_name,
            "role": old_role, "status": old_status,
        },
        "after": {
            "username": new_username, "nick_name": new_nick_name,
            "role": new_role, "status": new_status,
        },
    });
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, target_id, detail)
           VALUES ($1, 'ADMIN', 'USER_UPDATE', 'USER', $2, $3)"#,
    )
    .bind(admin.user_id)
    .bind(user_id)
    .bind(&detail)
    .execute(&mut *tx)
    .await;

    tx.commit().await?;

    // 外部副作用必须在事务提交后执行，避免回滚时误撤销 token。
    match status_action {
        PostCommitStatusAction::RevokeAndClearCache => {
            // 即使其中一个 Redis 操作失败，也尝试完成另一个。
            let revoke_result = jwt::set_user_revoked_at(user_id, &state.redis_cache).await;
            let cache_result = state.redis_cache.delete_user(&user_id.to_string()).await;
            revoke_result?;
            cache_result?;
        }
        PostCommitStatusAction::ClearCache => {
            // 恢复 ACTIVE 时不能复用封禁/注销前留下的 UserPublic。
            state.redis_cache.delete_user(&user_id.to_string()).await?;
        }
        PostCommitStatusAction::None => {}
    }

    let out: (i64, String, Option<String>, String, String, i32, i32, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            r#"SELECT user_id, username, nick_name, role::text, status::text, love_point, diamond, created_at
               FROM users WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;

    Ok(ApiResponse::success(UserOut {
        user_id: out.0,
        username: out.1,
        nick_name: out.2,
        role: out.3,
        status: out.4,
        love_point: out.5,
        diamond: out.6,
        created_at: out.7,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_input() -> UserUpdateInput {
        UserUpdateInput {
            username: None,
            nick_name: None,
            role: None,
            status: None,
        }
    }

    #[test]
    fn validate_user_update_all_none_rejected() {
        let err = validate_user_update(&empty_input()).unwrap_err();
        assert!(format!("{}", err).contains("至少需要更新一个字段"));
    }

    #[test]
    fn validate_user_update_username_empty_rejected() {
        let input = UserUpdateInput {
            username: Some(String::new()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("username 不能为空"));
    }

    #[test]
    fn validate_user_update_username_too_long_rejected() {
        let input = UserUpdateInput {
            username: Some("a".repeat(USERNAME_MAX_LEN + 1)),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("username 长度"));
    }

    #[test]
    fn validate_user_update_nickname_empty_rejected() {
        let input = UserUpdateInput {
            nick_name: Some(String::new()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("nickName 不能为空"));
    }

    #[test]
    fn validate_user_update_role_invalid_rejected() {
        let input = UserUpdateInput {
            role: Some("SuperAdmin".to_string()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("role 必须是"));
    }

    #[test]
    fn validate_user_update_role_valid_passes() {
        let input = UserUpdateInput {
            role: Some("ORDERING".to_string()),
            ..empty_input()
        };
        assert!(validate_user_update(&input).is_ok());
    }

    #[test]
    fn validate_user_update_status_invalid_rejected() {
        let input = UserUpdateInput {
            status: Some("banned".to_string()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("status 必须是"));
    }

    #[test]
    fn validate_user_update_status_valid_passes() {
        let input = UserUpdateInput {
            status: Some("ACTIVE".to_string()),
            ..empty_input()
        };
        assert!(validate_user_update(&input).is_ok());
    }

    #[test]
    fn active_to_inactive_revokes_and_clears_cache() {
        for status in ["BANNED", "DELETED"] {
            assert_eq!(
                post_commit_status_action("ACTIVE", status),
                PostCommitStatusAction::RevokeAndClearCache
            );
        }
    }

    #[test]
    fn restoring_active_only_clears_cache() {
        for status in ["BANNED", "DELETED"] {
            assert_eq!(
                post_commit_status_action(status, "ACTIVE"),
                PostCommitStatusAction::ClearCache
            );
        }
    }

    #[test]
    fn unchanged_status_has_no_side_effect() {
        assert_eq!(
            post_commit_status_action("ACTIVE", "ACTIVE"),
            PostCommitStatusAction::None
        );
    }
}
