use crate::{
    config::AppState,
    errors::CustomError,
    models::pagination::{decode_cursor, encode_cursor, CursorPage},
    models::sweet_talk::{SweetTalkCursor, SweetTalkOut, SweetTalkQuery, SweetTalkRequest},
    models::users::UserToken,
};
use chrono::Local;
use ntex::web::{
    types::{Json, Query, State},
    HttpResponse, Responder,
};
use std::sync::Arc;

#[utoipa::path(
    post,
    path = "/users/sweet-talk",
    tag = "用户",
    summary = "发表每日情话",
    request_body = SweetTalkRequest,
    responses(
        (status = 200, description = "发表成功"),
        (status = 400, description = "今日已发表或未绑定"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn add_sweet_talk(
    token: UserToken,
    data: Json<SweetTalkRequest>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id as i64;
    let content = data.content.trim();

    if content.is_empty() {
        return Err(CustomError::bad_request("内容不能为空"));
    }

    // 1. 检查是否在有效 PAIR 组中
    let group_id = sqlx::query_scalar::<_, i64>(
        "SELECT g.group_id FROM association_groups g
         JOIN association_group_members m ON g.group_id = m.group_id
         WHERE m.user_id = $1 AND g.group_type = 'PAIR' AND g.status = 1
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::bad_request("未找到有效的绑定关系，请先绑定另一半"))?;

    // 2. 检查今日是否已发表
    let today = Local::now().date_naive();
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT talk_id FROM sweet_talks WHERE user_id = $1 AND created_at::DATE = $2 LIMIT 1",
    )
    .bind(user_id)
    .bind(today)
    .fetch_optional(db)
    .await?;

    if exists.is_some() {
        return Err(CustomError::bad_request("今日已发表过情话，明天再来吧"));
    }

    // 3. 插入情话
    let talk_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO sweet_talks (user_id, group_id, content) VALUES ($1, $2, $3) RETURNING talk_id"
    )
    .bind(user_id)
    .bind(group_id)
    .bind(content)
    .fetch_one(db)
    .await?;

    /*
    // 3. 开始事务：插入情话 + 奖励积分
    let mut tx = db.begin().await?;

    // 获取当前积分
    let current_points: i32 = sqlx::query_scalar("SELECT love_point FROM users WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

    let reward_points = 2; // 固定奖励 2 积分
    let new_balance = current_points + reward_points;

    // 插入情话
    let talk_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO sweet_talks (user_id, group_id, content) VALUES ($1, $2, $3) RETURNING talk_id"
    )
    .bind(user_id)
    .bind(group_id)
    .bind(content)
    .fetch_one(&mut *tx)
    .await?;

    // 更新用户积分
    sqlx::query("UPDATE users SET love_point = love_point + $1 WHERE user_id = $2")
        .bind(reward_points)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    // 插入积分流水
    sqlx::query(
        "INSERT INTO point_transactions (user_id, amount, type, ref_type, ref_id, balance_after)
         VALUES ($1, $2, 'SWEET_TALK_REWARD', 2, $3, $4)"
    )
    .bind(user_id)
    .bind(reward_points)
    .bind(talk_id)
    .bind(new_balance)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    */

    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "talkId": talk_id,
        "message": "发表成功"
    })))
}

#[utoipa::path(
    put,
    path = "/users/sweet-talk/{id}",
    tag = "用户",
    summary = "编辑每日情话",
    request_body = SweetTalkRequest,
    params(
        ("id" = i64, Path, description = "情话 ID")
    ),
    responses(
        (status = 200, description = "修改成功"),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权编辑"),
        (status = 404, description = "情话不存在"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_sweet_talk(
    token: UserToken,
    id: ntex::web::types::Path<i64>,
    data: Json<SweetTalkRequest>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id as i64;
    let talk_id = id.into_inner();
    let content = data.content.trim();

    if content.is_empty() {
        return Err(CustomError::bad_request("内容不能为空"));
    }

    // 检查是否存在且属于该用户
    let owner_id =
        sqlx::query_scalar::<_, i64>("SELECT user_id FROM sweet_talks WHERE talk_id = $1")
            .bind(talk_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| CustomError::not_found("情话不存在"))?;

    if owner_id != user_id {
        return Err(CustomError::forbidden("无权编辑他人的情话"));
    }

    sqlx::query("UPDATE sweet_talks SET content = $1 WHERE talk_id = $2")
        .bind(content)
        .bind(talk_id)
        .execute(db)
        .await?;

    Ok(HttpResponse::Ok().json(&serde_json::json!({
        "message": "修改成功"
    })))
}

#[utoipa::path(
    get,
    path = "/users/sweet-talks",
    tag = "用户",
    summary = "获取情话历史",
    params(SweetTalkQuery),
    responses(
        (status = 200, body = CursorPage<SweetTalkOut>),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_sweet_talks(
    token: UserToken,
    query: Query<SweetTalkQuery>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id as i64;
    let limit = query.pagination.limit.unwrap_or(20).min(100);

    // 确定查询的组
    let group_id = if let Some(gid) = query.group_id {
        // 校验权限：用户是否在该组
        let is_member = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM association_group_members WHERE group_id = $1 AND user_id = $2",
        )
        .bind(gid)
        .bind(user_id)
        .fetch_optional(db)
        .await?;

        if is_member.is_none() {
            return Err(CustomError::forbidden("无权访问该组的情话历史"));
        }
        gid
    } else {
        // 默认查询用户的主 PAIR 组
        sqlx::query_scalar::<_, i64>(
            "SELECT g.group_id FROM association_groups g
             JOIN association_group_members m ON g.group_id = m.group_id
             WHERE m.user_id = $1 AND g.group_type = 'PAIR' AND g.status = 1
             ORDER BY m.is_primary DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| CustomError::bad_request("未找到绑定关系"))?
    };

    let cursor = query
        .pagination
        .cursor
        .as_deref()
        .and_then(decode_cursor::<SweetTalkCursor>);

    let mut sql = String::from(
        "SELECT st.talk_id, st.user_id, st.content, st.created_at, u.nick_name, u.avatar
         FROM sweet_talks st
         JOIN users u ON st.user_id = u.user_id
         WHERE st.group_id = $1",
    );

    let mut bind_idx = 2;
    if cursor.is_some() {
        sql.push_str(&format!(
            " AND (st.created_at, st.talk_id) < (${}, ${})",
            bind_idx,
            bind_idx + 1
        ));
        bind_idx += 2;
    }

    sql.push_str(" ORDER BY st.created_at DESC, st.talk_id DESC LIMIT $");
    sql.push_str(&bind_idx.to_string());

    let mut q = sqlx::query_as::<_, SweetTalkOut>(&sql).bind(group_id);
    if let Some(c) = cursor {
        q = q.bind(c.created_at).bind(c.talk_id);
    }
    q = q.bind(limit + 1);

    let mut items = q.fetch_all(db).await?;
    let has_more = items.len() > limit as usize;
    if has_more {
        items.pop();
    }

    let next_cursor = if has_more {
        items.last().map(|last| {
            encode_cursor(&SweetTalkCursor {
                created_at: last.created_at,
                talk_id: last.talk_id,
            })
        })
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(&CursorPage {
        items,
        next_cursor,
        has_more,
        total: None,
    }))
}
