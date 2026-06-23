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

/// PATCH /api/admin/groups/{group_id}
///
/// 管理员修改双人组名。空名 / 超 64 → 400，不存在 → 404。
/// 每次 PATCH 写 audit_logs。
#[utoipa::path(
    patch,
    path = "/api/admin/groups/{group_id}",
    tag = "后台管理 - 双人组",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = GroupUpdateInput,
    responses(
        (status = 200, description = "更新成功", body = GroupOut),
        (status = 400, description = "groupName 非法"),
        (status = 401, description = "未登录"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_group(
    state: State<Arc<AppState>>,
    admin: crate::middlewares::admin_auth::AdminToken,
    path: Path<i64>,
    body: Json<GroupUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    validate_group_update(&input)?;
    let new_name = input.group_name.unwrap().trim().to_string();

    let mut tx = db.begin().await?;

    let old_name: Option<String> = sqlx::query_scalar(
        "SELECT group_name FROM association_groups WHERE group_id = $1 FOR UPDATE",
    )
    .bind(group_id)
    .fetch_optional(&mut *tx)
    .await?;

    let old_name = match old_name {
        Some(n) => n,
        None => return Err(CustomError::NotFound("组不存在".into())),
    };

    sqlx::query(
        "UPDATE association_groups SET group_name = $1, updated_at = NOW() WHERE group_id = $2",
    )
    .bind(&new_name)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;

    let detail = serde_json::json!({
        "group_id": group_id,
        "before": { "group_name": old_name },
        "after": { "group_name": new_name },
    });
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, target_id, detail)
           VALUES ($1, 'ADMIN', 'GROUP_UPDATE', 'ASSOCIATION_GROUP', $2, $3)"#,
    )
    .bind(admin.user_id)
    .bind(group_id)
    .bind(&detail)
    .execute(&mut *tx)
    .await;

    tx.commit().await?;

    let out: (String, i64, i32, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        r#"SELECT group_name, diamond, member_count, created_at
           FROM association_groups WHERE group_id = $1"#,
    )
    .bind(group_id)
    .fetch_one(db)
    .await?;

    Ok(ApiResponse::success(GroupOut {
        group_id,
        group_name: out.0,
        diamond: out.1,
        member_count: out.2,
        created_at: out.3,
    }))
}

/// GET /api/admin/groups/{group_id}/members
///
/// 返回 ACTIVE 成员列表，按 is_primary DESC, joined_at ASC。
/// 不存在 → 404。
#[utoipa::path(
    get,
    path = "/api/admin/groups/{group_id}/members",
    tag = "后台管理 - 双人组",
    params(("group_id" = i64, Path, description = "组 ID")),
    responses(
        (status = 200, description = "成员列表", body = Vec<GroupMember>),
        (status = 401, description = "未登录"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_group_members(
    state: State<Arc<AppState>>,
    _admin: crate::middlewares::admin_auth::AdminToken,
    path: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let db = &state.db_pool;

    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT group_id FROM association_groups WHERE group_id = $1",
    )
    .bind(group_id)
    .fetch_optional(db)
    .await?;

    if exists.is_none() {
        return Err(CustomError::NotFound("组不存在".into()));
    }

    let rows = sqlx::query(
        r#"SELECT m.user_id, u.username, u.nick_name, m.is_primary,
                  m.role_in_group::text, m.member_status::text, m.joined_at
           FROM association_group_members m
           JOIN users u ON u.user_id = m.user_id
           WHERE m.group_id = $1 AND m.member_status = 'ACTIVE'
           ORDER BY m.is_primary DESC, m.joined_at ASC"#,
    )
    .bind(group_id)
    .fetch_all(db)
    .await?;

    let members: Vec<GroupMember> = rows
        .iter()
        .map(|r| GroupMember {
            user_id: r.get("user_id"),
            username: r.get("username"),
            nick_name: r.get("nick_name"),
            is_primary: r.get::<i16, _>("is_primary") != 0,
            role_in_group: r.get("role_in_group"),
            member_status: r.get("member_status"),
            joined_at: r.get("joined_at"),
        })
        .collect();

    Ok(ApiResponse::success(members))
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