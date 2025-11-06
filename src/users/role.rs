use crate::{
    errors::CustomError,
    models::users::{UserRoleEnum, UserToken},
    AppState,
};
use chrono::{Duration, Utc};
use ntex::web::{
    types::{Json, State},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RoleSwitchInput {
    pub group_id: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RoleSwitchResult {
    pub group_id: i64,
    pub switched_at: chrono::DateTime<chrono::Utc>,
    pub user_id: i64,
    pub new_role: UserRoleEnum,
    pub counterpart_user_id: i64,
    pub counterpart_new_role: UserRoleEnum,
    pub next_allowed_switch_at: chrono::DateTime<chrono::Utc>,
}

const SWITCH_COOLDOWN_MONTHS: i64 = 6; // 半年冷却

#[utoipa::path(post, path="/users/role-switch", tag="用户", request_body=RoleSwitchInput, responses((status=200, body=RoleSwitchResult)), security(("cookie_auth"=[])))]
pub async fn switch_role(
    token: UserToken,
    state: State<Arc<AppState>>,
    body: Json<RoleSwitchInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    // 查找目标 group: 如果提供 group_id 则验证；否则自动找一个 pair 组
    let group_row_opt = if let Some(gid) = body.group_id {
        sqlx::query("SELECT group_id, group_type FROM association_groups WHERE group_id=$1")
            .bind(gid)
            .fetch_optional(db)
            .await?
    } else {
        sqlx::query("SELECT g.group_id, g.group_type FROM association_groups g JOIN association_group_members m ON g.group_id=m.group_id WHERE m.user_id=$1 AND g.group_type='PAIR' LIMIT 1")
            .bind(token.user_id)
            .fetch_optional(db).await?
    };
    let Some(group_row) = group_row_opt else {
        return Err(CustomError::BadRequest("未找到可用的PAIR关联组".into()));
    };
    let group_id: i64 = group_row.get("group_id");
    let gtype: String = group_row.get("group_type");
    if gtype != "PAIR" {
        return Err(CustomError::BadRequest("仅支持PAIR类型组内角色互换".into()));
    }

    // 取组内两个成员
    let members = sqlx::query("SELECT user_id, role_in_group FROM association_group_members WHERE group_id=$1 ORDER BY user_id")
        .bind(group_id)
        .fetch_all(db).await?;
    if members.len() != 2 {
        return Err(CustomError::BadRequest("组成员数量必须为2".into()));
    }

    // 找当前用户与对方
    let mut self_role: Option<String> = None;
    let mut counterpart: Option<(i64, String)> = None;
    for r in &members {
        let uid: i64 = r.get("user_id");
        let role: String = r.get("role_in_group");
        if uid == token.user_id {
            self_role = Some(role);
        } else {
            counterpart = Some((uid, role));
        }
    }
    let Some(self_role_str) = self_role else {
        return Err(CustomError::BadRequest("当前用户不在该组".into()));
    };
    let Some((cp_uid, cp_role_str)) = counterpart else {
        return Err(CustomError::BadRequest("未找到对方成员".into()));
    };

    if self_role_str == cp_role_str {
        return Err(CustomError::BadRequest("双方角色相同，无法互换".into()));
    }
    if !(self_role_str == "ORDERING" || self_role_str == "RECEIVING") {
        return Err(CustomError::BadRequest("当前角色不支持互换".into()));
    }

    // 冷却校验（自身）
    let cooldown_row = sqlx::query("SELECT last_role_switch_at FROM users WHERE user_id=$1")
        .bind(token.user_id)
        .fetch_one(db)
        .await?;
    let last_switch: Option<chrono::DateTime<chrono::Utc>> =
        cooldown_row.try_get("last_role_switch_at").ok();
    if let Some(ts) = last_switch {
        if Utc::now() < ts + Duration::days(30 * SWITCH_COOLDOWN_MONTHS) {
            return Err(CustomError::BadRequest("角色切换仍在冷却期".into()));
        }
    }

    // 目标角色
    let new_self_role = if self_role_str == "ORDERING" {
        "RECEIVING"
    } else {
        "ORDERING"
    };
    let new_cp_role = if cp_role_str == "ORDERING" {
        "RECEIVING"
    } else {
        "ORDERING"
    };

    // 开启事务并同时更新 association_group_members 与 users.role, 以及 last_role_switch_at
    let mut tx = db.begin().await?;
    // 更新组内角色
    sqlx::query(
        "UPDATE association_group_members SET role_in_group=$3 WHERE group_id=$1 AND user_id=$2",
    )
    .bind(group_id)
    .bind(token.user_id)
    .bind(new_self_role)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE association_group_members SET role_in_group=$3 WHERE group_id=$1 AND user_id=$2",
    )
    .bind(group_id)
    .bind(cp_uid)
    .bind(new_cp_role)
    .execute(&mut *tx)
    .await?;
    // 更新全局用户角色
    sqlx::query(
        "UPDATE users SET role=$2, last_role_switch_at=NOW(), updated_at=NOW() WHERE user_id=$1",
    )
    .bind(token.user_id)
    .bind(new_self_role)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE users SET role=$2, last_role_switch_at=NOW(), updated_at=NOW() WHERE user_id=$1",
    )
    .bind(cp_uid)
    .bind(new_cp_role)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let switched_at = Utc::now();
    let next_allowed = switched_at + Duration::days(30 * SWITCH_COOLDOWN_MONTHS);
    let result = RoleSwitchResult {
        group_id,
        switched_at,
        user_id: token.user_id,
        new_role: if new_self_role == "ORDERING" {
            UserRoleEnum::ORDERING
        } else {
            UserRoleEnum::RECEIVING
        },
        counterpart_user_id: cp_uid,
        counterpart_new_role: if new_cp_role == "ORDERING" {
            UserRoleEnum::ORDERING
        } else {
            UserRoleEnum::RECEIVING
        },
        next_allowed_switch_at: next_allowed,
    };
    Ok(HttpResponse::Ok().json(&result))
}
