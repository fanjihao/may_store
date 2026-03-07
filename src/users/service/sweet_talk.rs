use crate::{
    config::AppState,
    errors::CustomError,
    models::pagination::{decode_cursor, encode_cursor, CursorPage},
    users::models::sweet_talk::{SweetTalkCursor, SweetTalkOut, SweetTalkQuery, SweetTalkRequest},
};
use chrono::Local;

pub struct SweetTalkService;

impl SweetTalkService {
    pub async fn add_sweet_talk(user_id: i64, data: SweetTalkRequest, state: &AppState) -> Result<i64, CustomError> {
    let db = &state.db_pool;
    let content = data.content.trim();

    if content.is_empty() {
        return Err(CustomError::bad_request("内容不能为空"));
    }

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

    let talk_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO sweet_talks (user_id, group_id, content) VALUES ($1, $2, $3) RETURNING talk_id"
    )
    .bind(user_id)
    .bind(group_id)
    .bind(content)
    .fetch_one(db)
    .await?;

    Ok(talk_id)
}

    pub async fn update_sweet_talk(user_id: i64, talk_id: i64, data: SweetTalkRequest, state: &AppState) -> Result<(), CustomError> {
    let db = &state.db_pool;
    let content = data.content.trim();

    if content.is_empty() {
        return Err(CustomError::bad_request("内容不能为空"));
    }

    let owner_id = sqlx::query_scalar::<_, i64>("SELECT user_id FROM sweet_talks WHERE talk_id = $1")
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

    Ok(())
}

    pub async fn get_sweet_talks(user_id: i64, query: SweetTalkQuery, state: &AppState) -> Result<CursorPage<SweetTalkOut>, CustomError> {
    let db = &state.db_pool;
    let limit = query.pagination.limit.unwrap_or(20).min(100);

    let group_id = if let Some(gid) = query.group_id {
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

    Ok(CursorPage {
        items,
        next_cursor,
        has_more,
        total: None,
    })
}}
