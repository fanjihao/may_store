use crate::{
    AppState, errors::CustomError, models::{
        invitation::{
            ConfirmInvitationInput, GroupInfoOut, GroupMemberOut, InvitationListOut, InvitationRequestOut, NewInvitationInput, UnbindRequestInput
        },
        users::{UserRoleEnum, UserToken},
    }
};
use chrono::Utc;
use ntex::web::{
    types::{Json, Path, State},
    HttpResponse, Responder,
};
use sqlx::FromRow;
use std::sync::Arc;
use sqlx::Row;

// 内部查询结构体（不暴露到 OpenAPI）
#[derive(FromRow)]
struct RequestRow {
    request_id: i64,
    requester_id: i64,
    target_user_id: i64,
    status: i16,
}
#[derive(FromRow)]
struct RoleRow {
    user_id: i64,
    role: UserRoleEnum,
}
#[derive(FromRow)]
struct CancelRow {
    request_id: i64,
    status: i16,
}

#[utoipa::path(
    get,
    path = "/invitation",
    tag = "用户",
    summary = "获取当前用户的邀请列表（incoming/outgoing）",
    responses(
        (status = 200, body = InvitationListOut),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_invitation(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let uid = token.user_id;
    let outgoing = sqlx::query_as::<_, InvitationRequestOut>(
        "SELECT 
            request_id, 
            requester_id, 
            u.username as requester_username,
            u.avatar as requester_avatar,
            target_user_id, 
            agr.status,
            remark, 
            agr.created_at, 
            handled_at 
        FROM
            association_group_requests agr
        LEFT JOIN users u ON u.user_id = target_user_id
        WHERE 
            requester_id = $1 AND agr.status IN (0, 4)
        ORDER BY 
            request_id DESC"
    )
    .bind(uid)
    .fetch_all(db)
    .await?;

    let incoming = sqlx::query_as::<_, InvitationRequestOut>(
        "SELECT
            request_id,
            requester_id,
            u.username as requester_username,
            u.avatar as requester_avatar,
            target_user_id,
            agr.status,
            remark,
            agr.created_at,
            handled_at
        FROM
            association_group_requests agr
        LEFT JOIN users u ON u.user_id = requester_id
        WHERE
            target_user_id = $1 AND agr.status IN (0, 4)
        ORDER BY
            target_user_id DESC"
    )
    .bind(uid)
    .fetch_all(db)
    .await?;

    Ok(Json(InvitationListOut { incoming, outgoing }))
}

#[utoipa::path(
    post,
    path = "/invitation",
    tag = "用户",
    summary = "发起绑定邀请",
    request_body = NewInvitationInput,
    responses(
        (status = 201, description = "邀请成功，无响应体"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn new_invitation(
    token: UserToken,
    data: Json<NewInvitationInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let uid = token.user_id;
    let target = data.target_user_id;
    if uid == target {
        return Err(CustomError::BadRequest("不能邀请自己".into()));
    }

    let target_exists =
        sqlx::query_scalar::<_, i64>("SELECT user_id FROM users WHERE user_id = $1 AND status = 1")
            .bind(target)
            .fetch_optional(db)
            .await?;
    if target_exists.is_none() {
        return Err(CustomError::BadRequest("目标用户不存在或被禁用".into()));
    }

    let exists_pending = sqlx::query_scalar::<_, i64>(
        "SELECT request_id FROM association_group_requests WHERE ((requester_id=$1 AND target_user_id=$2) OR (requester_id=$2 AND target_user_id=$1)) AND status = 0"
    )
    .bind(uid)
    .bind(target)
    .fetch_optional(db)
    .await?;
    if exists_pending.is_some() {
        return Err(CustomError::BadRequest("已存在待处理邀请".into()));
    }

    let paired = sqlx::query_scalar::<_, i64>(
        "SELECT ag.group_id FROM association_groups ag JOIN association_group_members m1 ON ag.group_id = m1.group_id JOIN association_group_members m2 ON ag.group_id = m2.group_id WHERE ag.group_type='PAIR' AND m1.user_id=$1 AND m2.user_id=$2 LIMIT 1"
    )
    .bind(uid)
    .bind(target)
    .fetch_optional(db)
    .await?;
    if paired.is_some() {
        return Err(CustomError::BadRequest("已绑定，不能重复邀请".into()));
    }

    sqlx::query(
        r#"INSERT INTO association_group_requests (requester_id, target_user_id, remark) VALUES ($1,$2,$3)"#
    )
    .bind(uid)
    .bind(target)
    .bind(data.remark.clone())
    .execute(db)
    .await?;

    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    put,
    path = "/invitation/{id}",
    tag = "用户",
    summary = "确认或拒绝邀请 (accept=true 同意)",
    params(("id" = i64, Path, description = "邀请ID")),
    request_body = ConfirmInvitationInput,
    responses(
        (status = 200, description = "操作成功，无响应体"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn confirm_invitation(
    token: UserToken,
    id: Path<(i64,)>,
    data: Json<ConfirmInvitationInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;
    // 读取请求并锁定行，确保并发安全
    let req_opt = sqlx::query_as::<_, RequestRow>(
        "SELECT request_id, requester_id, target_user_id, status FROM association_group_requests WHERE request_id=$1 FOR UPDATE"
    )
    .bind(id.0)
    .fetch_optional(&mut *tx)
    .await?;
    let req = match req_opt {
        Some(r) => r,
        None => {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("邀请不存在".into()));
        }
    };
    if req.target_user_id != token.user_id {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("无权限操作该邀请".into()));
    }
    // status 0=待处理邀请, 4=申请解绑中
    if req.status != 0 && req.status != 4 {
        tx.rollback().await.ok();
        return Err(CustomError::BadRequest("邀请已处理".into()));
    }

    // 判断当前状态：0=待处理绑定邀请, 4=申请解绑中
    if data.accept {
        // 同意
        if req.status == 4 {
            // 状态4是解绑申请，同意后：改为状态5(已解绑)，删除group信息
            let now = Utc::now();
            sqlx::query(
                "UPDATE association_group_requests SET status=5, handled_at=$1 WHERE request_id=$2"
            )
            .bind(now)
            .bind(req.request_id)
            .execute(&mut *tx)
            .await?;
            
            // 查找两人所在的 PAIR 组并删除
            let pair_id = sqlx::query_scalar::<_, Option<i64>>(
                "SELECT ag.group_id FROM association_groups ag \
                 JOIN association_group_members m1 ON ag.group_id = m1.group_id \
                 JOIN association_group_members m2 ON ag.group_id = m2.group_id \
                 WHERE ag.group_type='PAIR' AND m1.user_id=$1 AND m2.user_id=$2 AND ag.status=1 LIMIT 1"
            )
            .bind(req.requester_id)
            .bind(req.target_user_id)
            .fetch_optional(&mut *tx)
            .await?.flatten();
            
            if let Some(gid) = pair_id {
                sqlx::query("DELETE FROM association_group_members WHERE group_id=$1")
                    .bind(gid)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE association_groups SET status=0, updated_at=NOW() WHERE group_id=$1")
                    .bind(gid)
                    .execute(&mut *tx)
                    .await?;
            }
            
            tx.commit().await?;
            return Ok(HttpResponse::Ok().finish());
        }
        // 状态0是绑定邀请，同意后：更新status=1，查是否已存在配对组
        let now = Utc::now();
        sqlx::query(
            "UPDATE association_group_requests SET status=1, handled_at=$1 WHERE request_id=$2",
        )
        .bind(now)
        .bind(req.request_id)
        .execute(&mut *tx)
        .await?;
        let existing_pair = sqlx::query_scalar::<_, i64>(
            "SELECT ag.group_id FROM association_groups ag JOIN association_group_members m1 ON ag.group_id = m1.group_id JOIN association_group_members m2 ON ag.group_id = m2.group_id WHERE ag.group_type='PAIR' AND m1.user_id=$1 AND m2.user_id=$2 LIMIT 1"
        )
        .bind(req.requester_id)
        .bind(req.target_user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let group_id = if let Some(gid) = existing_pair {
            gid
        } else {
            let group_name = format!("pair-{}-{}", req.requester_id, req.target_user_id);
            // 创建组
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO association_groups (group_name, group_type) VALUES ($1,'PAIR') RETURNING group_id"
            )
            .bind(group_name)
            .fetch_one(&mut *tx)
            .await?
        };
        // 分别读取两个用户角色，为防止 IN ($1,$2) 兼容性问题，拆成两个查询
        let role_req =
            sqlx::query_as::<_, RoleRow>("SELECT user_id, role FROM users WHERE user_id=$1")
                .bind(req.requester_id)
                .fetch_optional(&mut *tx)
                .await?;
        let role_tgt =
            sqlx::query_as::<_, RoleRow>("SELECT user_id, role FROM users WHERE user_id=$1")
                .bind(req.target_user_id)
                .fetch_optional(&mut *tx)
                .await?;
        let mut role_rows: Vec<RoleRow> = Vec::new();
        if let Some(r) = role_req {
            role_rows.push(r);
        }
        if let Some(r) = role_tgt {
            role_rows.push(r);
        }
        if role_rows.len() != 2 {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("用户角色读取失败".into()));
        }
        // 判断是否需要自动补齐一对互补角色（双方都 ORDERING 或都 RECEIVING 时）
        let req_role_enum = role_rows
            .iter()
            .find(|r| r.user_id == req.requester_id)
            .unwrap()
            .role;
        let tgt_role_enum = role_rows
            .iter()
            .find(|r| r.user_id == req.target_user_id)
            .unwrap()
            .role;
        let mut adjusted_target_role: Option<UserRoleEnum> = None;
        if matches!(req_role_enum, UserRoleEnum::ORDERING)
            && matches!(tgt_role_enum, UserRoleEnum::ORDERING)
        {
            adjusted_target_role = Some(UserRoleEnum::RECEIVING);
        } else if matches!(req_role_enum, UserRoleEnum::RECEIVING)
            && matches!(tgt_role_enum, UserRoleEnum::RECEIVING)
        {
            adjusted_target_role = Some(UserRoleEnum::ORDERING);
        }
        if let Some(new_role) = adjusted_target_role {
            // 只调整被邀请者（target），保持发起者不变，未写 last_role_switch_at 因为这不是一次互换，只是初次补齐
            sqlx::query("UPDATE users SET role=$2::user_role_enum, updated_at=NOW() WHERE user_id=$1")
                .bind(req.target_user_id)
                .bind(match new_role {
                    UserRoleEnum::ORDERING => "ORDERING",
                    UserRoleEnum::RECEIVING => "RECEIVING",
                    UserRoleEnum::ADMIN => "ADMIN",
                })
                .execute(&mut *tx)
                .await?;
        }
        // 重新获取最终角色并写入组成员
        let final_req_role =
            sqlx::query_scalar::<_, String>("SELECT role::text FROM users WHERE user_id=$1")
                .bind(req.requester_id)
                .fetch_one(&mut *tx)
                .await?;
        let final_tgt_role =
            sqlx::query_scalar::<_, String>("SELECT role::text FROM users WHERE user_id=$1")
                .bind(req.target_user_id)
                .fetch_one(&mut *tx)
                .await?;
        for (uid, role_txt) in [
            (req.requester_id, final_req_role),
            (req.target_user_id, final_tgt_role),
        ] {
            sqlx::query(
                "INSERT INTO association_group_members (group_id, user_id, role_in_group, is_primary) VALUES ($1,$2,$3::group_member_role_enum,$4) ON CONFLICT (group_id, user_id) DO UPDATE SET role_in_group=EXCLUDED.role_in_group"
            )
            .bind(group_id)
            .bind(uid)
            .bind(role_txt)
            .bind(if uid == req.requester_id { 1 } else { 0 })
            .execute(&mut *tx)
            .await?;
        }
    } else {
        // 拒绝
        let now = Utc::now();
        if req.status == 4 {
            // 拒绝解绑申请：状态改回 1(已绑定)
            sqlx::query(
                "UPDATE association_group_requests SET status=1, handled_at=$1 WHERE request_id=$2",
            )
            .bind(now)
            .bind(req.request_id)
            .execute(&mut *tx)
            .await?;
        } else {
            // 拒绝绑定邀请：状态改为 2(已拒绝)
            sqlx::query(
                "UPDATE association_group_requests SET status=2, handled_at=$1 WHERE request_id=$2",
            )
            .bind(now)
            .bind(req.request_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    delete,
    path = "/invitation/{id}",
    tag = "用户",
    summary = "取消自己发起的待处理邀请",
    params(("id" = i64, Path, description = "邀请ID")),
    responses(
        (status = 200, body = InvitationRequestOut),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn cancel_invitation(
    _: UserToken,
    id: Path<(i64,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let mut tx = db.begin().await?;
    let req: CancelRow = sqlx::query_as::<_, CancelRow>(
        "SELECT request_id, requester_id, status FROM association_group_requests WHERE request_id=$1 FOR UPDATE"
    )
    .bind(id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| CustomError::BadRequest("邀请不存在".into()))?;
    if req.status != 0 && req.status != 4 {
        return Err(CustomError::BadRequest("该邀请已处理".into()));
    }
    let now = Utc::now();
    sqlx::query(
        "UPDATE association_group_requests SET status=3, handled_at=$2 WHERE request_id=$1",
    )
    .bind(req.request_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

#[utoipa::path(
    post,
    path = "/invitation/unbind",
    tag = "用户",
    summary = "申请解绑（需对方同意）",
    request_body = UnbindRequestInput,
    responses(
        (status = 200, description = "解绑申请已发起"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn unbind_request(
    token: UserToken,
    data: Json<UnbindRequestInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let uid = token.user_id;
    let target = data.target_user_id;
    if uid == target {
        return Err(CustomError::BadRequest("不能对自己发起解绑".into()));
    }
    // 查找现有的绑定关系 request (status=1 表示已绑定)
    let existing = sqlx::query(
        "SELECT request_id FROM association_group_requests \
         WHERE ((requester_id=$1 AND target_user_id=$2) OR (requester_id=$2 AND target_user_id=$1)) \
         AND status=1 LIMIT 1"
    )
    .bind(uid)
    .bind(target)
    .fetch_optional(db)
    .await?;
    
    if existing.is_none() {
        return Err(CustomError::BadRequest("未找到绑定关系".into()));
    }
    
    let request_id: i64 = existing.unwrap().get("request_id");
    
    // 将状态更新为 4 (申请解绑中)，并更新 requester 为当前用户
    sqlx::query(
        "UPDATE association_group_requests \
         SET status=4, requester_id=$1, target_user_id=$2, remark=$3, created_at=NOW() \
         WHERE request_id=$4"
    )
    .bind(uid)
    .bind(target)
    .bind(data.remark.clone())
    .bind(request_id)
    .execute(db)
    .await?;
    
    Ok(HttpResponse::Ok().finish())
}


#[utoipa::path(
    get,
    path = "/invitation/group/{id}",
    tag = "用户",
    summary = "获取群组详情及成员列表",
    params(("id" = i64, Path, description = "群组ID")),
    responses(
        (status = 200, body = GroupInfoOut),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_group_info(
    _token: UserToken,
    id: Path<(i64,)>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let base = sqlx::query(
        "SELECT group_id, group_name, group_type::text, status, created_at, updated_at FROM association_groups WHERE group_id=$1"
    )
    .bind(id.0)
    .fetch_optional(db)
    .await?;
    let g = match base { Some(r) => r, None => return Err(CustomError::BadRequest("群组不存在".into())) };
    let member_rows = sqlx::query(
        "SELECT agm.user_id, u.nick_name, u.avatar, agm.role_in_group::text, agm.is_primary FROM association_group_members agm LEFT JOIN users u ON u.user_id=agm.user_id WHERE agm.group_id=$1 ORDER BY agm.is_primary DESC, agm.user_id"
    )
    .bind(id.0)
    .fetch_all(db)
    .await?;
    let mut members = Vec::with_capacity(member_rows.len());
    for r in member_rows {
        members.push(GroupMemberOut {
            user_id: r.get("user_id"),
            nick_name: r.try_get("nick_name").ok(),
            avatar: r.try_get("avatar").ok(),
            role_in_group: r.try_get::<Option<String>, _>("role_in_group").ok().flatten(),
            is_primary: r.get("is_primary"),
        });
    }
    // 查询订单统计：组成员的所有订单
    let member_ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    let stats = if member_ids.is_empty() {
        (0i64, 0i64)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orders WHERE user_id = ANY($1)"
        )
        .bind(&member_ids)
        .fetch_one(db)
        .await.unwrap_or(0);
        let completed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orders WHERE user_id = ANY($1) AND status = 'FINISHED'"
        )
        .bind(&member_ids)
        .fetch_one(db)
        .await.unwrap_or(0);
        (total, completed)
    };
    let out = GroupInfoOut {
        group_id: g.get("group_id"),
        group_name: g.try_get("group_name").ok(),
        group_type: g.get("group_type"),
        status: g.get("status"),
        created_at: g.get("created_at"),
        updated_at: g.get("updated_at"),
        members,
        total_orders: stats.0,
        completed_orders: stats.1,
    };
    Ok(HttpResponse::Ok().json(&out))
}
