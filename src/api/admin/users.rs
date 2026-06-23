// API 层 - 后台用户管理

use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::utils::response::ApiResponse;

/// user_role_enum 合法值(查 v3.sql L332)
const VALID_ROLES: &[&str] = &["ORDERING", "RECEIVING", "ADMIN"];

/// user_status_enum 合法值(查 v3.sql L327)
const VALID_STATUSES: &[&str] = &["ACTIVE", "BANNED", "DELETED"];

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
        return Err(CustomError::BadRequest(
            "至少需要更新一个字段".into(),
        ));
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
}