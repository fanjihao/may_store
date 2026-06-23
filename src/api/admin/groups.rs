// API 层 - 后台双人组管理

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

/// 组名最大长度(查 association_groups.group_name VARCHAR(64))
const GROUP_NAME_MAX_LEN: usize = 64;

/// 组更新输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupUpdateInput {
    pub group_name: Option<String>,
}

/// 组输出(响应体)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupOut {
    pub group_id: i64,
    pub group_name: String,
    pub diamond: i64,
    pub member_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 组成员输出(GET members 响应体)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub user_id: i64,
    pub username: String,
    pub nick_name: Option<String>,
    pub is_primary: bool,
    pub role_in_group: String,
    pub member_status: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

/// 校验组更新输入
pub fn validate_group_update(input: &GroupUpdateInput) -> Result<(), CustomError> {
    if input.group_name.is_none() {
        return Err(CustomError::BadRequest(
            "至少需要更新一个字段(groupName)".into(),
        ));
    }
    let name = input.group_name.as_ref().unwrap().trim();
    if name.is_empty() {
        return Err(CustomError::BadRequest("groupName 不能为空".into()));
    }
    if name.len() > GROUP_NAME_MAX_LEN {
        return Err(CustomError::BadRequest(format!(
            "groupName 长度不能超过 {}",
            GROUP_NAME_MAX_LEN
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_group_update_none_rejected() {
        let input = GroupUpdateInput { group_name: None };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("至少需要更新一个字段"));
    }

    #[test]
    fn validate_group_update_empty_rejected() {
        let input = GroupUpdateInput {
            group_name: Some(String::new()),
        };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("groupName 不能为空"));
    }

    #[test]
    fn validate_group_update_whitespace_rejected() {
        let input = GroupUpdateInput {
            group_name: Some("   ".to_string()),
        };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("groupName 不能为空"));
    }

    #[test]
    fn validate_group_update_too_long_rejected() {
        let input = GroupUpdateInput {
            group_name: Some("a".repeat(GROUP_NAME_MAX_LEN + 1)),
        };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("groupName 长度"));
    }

    #[test]
    fn validate_group_update_valid_passes() {
        let input = GroupUpdateInput {
            group_name: Some("My Group".to_string()),
        };
        assert!(validate_group_update(&input).is_ok());
    }
}