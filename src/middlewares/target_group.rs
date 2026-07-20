//! 目标组授权守卫。
//!
//! 与 [`super::require_group::RequireGroup`] 不同，本模块始终以请求指定的
//! `group_id` 查询数据库，同时要求成员关系和目标组本身都处于 `ACTIVE` 状态。

use sqlx::{Executor, Postgres};

use crate::errors::CustomError;

fn active_target_group_access(member_status: Option<&str>, group_status: Option<&str>) -> bool {
    member_status == Some("ACTIVE") && group_status == Some("ACTIVE")
}

/// 查询调用者是否为目标组的活跃成员。
///
/// `executor` 可传 `&PgPool`，也可在事务中传 `&mut PgConnection`
/// （例如 `&mut *tx`），确保关键写操作能在同一事务内完成授权。
pub async fn is_active_target_group_member<'e, E>(
    executor: E,
    user_id: i64,
    group_id: i64,
) -> Result<bool, CustomError>
where
    E: Executor<'e, Database = Postgres>,
{
    let statuses: Option<(String, String)> = sqlx::query_as(
        r#"SELECT m.member_status::text, g.status::text
           FROM association_group_members m
           JOIN association_groups g ON g.group_id = m.group_id
           WHERE m.user_id = $1 AND m.group_id = $2"#,
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_optional(executor)
    .await?;

    Ok(statuses
        .as_ref()
        .map(|(member, group)| {
            active_target_group_access(Some(member.as_str()), Some(group.as_str()))
        })
        .unwrap_or(false))
}

/// 要求调用者为目标组的活跃成员，否则统一返回 403。
pub async fn require_active_target_group_member<'e, E>(
    executor: E,
    user_id: i64,
    group_id: i64,
) -> Result<(), CustomError>
where
    E: Executor<'e, Database = Postgres>,
{
    if is_active_target_group_member(executor, user_id, group_id).await? {
        Ok(())
    } else {
        Err(CustomError::Forbidden("无权访问该组".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::active_target_group_access;

    #[test]
    fn target_group_guard_requires_both_active_states() {
        assert!(active_target_group_access(Some("ACTIVE"), Some("ACTIVE")));
        assert!(!active_target_group_access(Some("LEFT"), Some("ACTIVE")));
        assert!(!active_target_group_access(
            Some("ACTIVE"),
            Some("DISABLED")
        ));
        assert!(!active_target_group_access(Some("LEFT"), Some("DISABLED")));
        assert!(!active_target_group_access(None, Some("ACTIVE")));
        assert!(!active_target_group_access(Some("ACTIVE"), None));
    }
}
