// API - 首页今日待办
// Spec: docs/superpowers/specs/2026-06-15-homepage-cards-design.md §2

use chrono::Local;
use ntex::web::{types::State, Responder};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

// ========== 响应 DTO ==========

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TodoType {
    SignIn,
    OrderAccept,
    OrderComplete,
    OrderConfirm,
    WishFulfill,
    WishNegotiate,
    UnreadNotifications,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub r#type: TodoType,
    pub priority: u8,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub ref_id: Option<i64>,
    pub action_url: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayTodosSummary {
    pub sign_in_signed: bool,
    pub sign_in_groups_pending: i32,
    pub orders_to_handle: i32,
    pub wishes_to_handle: i32,
    pub unread_count: i32,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayTodosResponse {
    pub date: String,
    pub items: Vec<TodoItem>,
    pub summary: TodayTodosSummary,
}

// ========== 内部行结构 ==========

struct SignInRow {
    group_id: i64,
    group_name: String,
    signed_today: bool,
    consecutive_days: i32,
}

struct OrderRow {
    kind: String,
    ref_id: i64,
    group_id: i64,
    group_name: String,
    /// orders.title 是可空列(VARCHAR(128)), 老数据可能为 NULL
    title: Option<String>,
}

struct WishRow {
    kind: String,
    ref_id: i64,
    group_id: i64,
    group_name: String,
    title: String,
}

// ========== Handler ==========

/// GET /api/users/me/today-todos
#[utoipa::path(
    get,
    path = "/api/users/me/today-todos",
    tag = "用户",
    responses(
        (status = 200, description = "获取成功", body = TodayTodosResponse),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_today_todos(
    state: State<Arc<AppState>>,
    token: UserToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let user_id = token.user_id;
    // ASSUMPTION: server runs in Asia/Shanghai (UTC+8). If the deployment
    // ever moves to UTC or another zone, the "today" day boundary shifts
    // and the 今日待办 list may include yesterday's not-yet-cleared tasks
    // (or miss today's). Pin the timezone in deployment config, or switch
    // to a fixed offset (e.g. FixedOffset::east_opt(8*3600)) if you want
    // the code itself to be timezone-explicit.
    let today = Local::now().date_naive();
    let today_str = today.to_string();

    let (sign_in_rows, order_rows, wish_rows, unread_count): (
        Vec<SignInRow>,
        Vec<OrderRow>,
        Vec<WishRow>,
        i64,
    ) = tokio::try_join!(
        fetch_sign_in(db, user_id, today),
        fetch_orders(db, user_id),
        fetch_wishes(db, user_id),
        async {
            Ok::<i64, sqlx::Error>(
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false",
                )
                .bind(user_id)
                .fetch_one(db)
                .await?,
            )
        },
    )?;

    let mut items: Vec<TodoItem> = Vec::new();

    let mut sign_in_signed = false;
    for r in &sign_in_rows {
        if r.signed_today {
            sign_in_signed = true;
        } else {
            items.push(TodoItem {
                r#type: TodoType::SignIn,
                priority: 1,
                group_id: Some(r.group_id),
                group_name: Some(r.group_name.clone()),
                title: "今日签到".to_string(),
                subtitle: Some(if r.consecutive_days > 0 {
                    format!("已连续签到 {} 天", r.consecutive_days)
                } else {
                    "快来和 TA 一起签到吧".to_string()
                }),
                ref_id: None,
                action_url: format!("/pages/sign-in/index?groupId={}", r.group_id),
            });
        }
    }

    for r in &order_rows {
        let (t, title) = order_kind_to_todo(&r.kind);
        items.push(TodoItem {
            r#type: t,
            priority: 2,
            group_id: Some(r.group_id),
            group_name: Some(r.group_name.clone()),
            title: title.to_string(),
            // orders.title 可空, NULL 时显示占位
            subtitle: Some(r.title.clone().unwrap_or_else(|| "（未命名）".to_string())),
            ref_id: Some(r.ref_id),
            action_url: format!("/pages/orders/detail?id={}", r.ref_id),
        });
    }

    for r in &wish_rows {
        let (t, title) = wish_kind_to_todo(&r.kind);
        items.push(TodoItem {
            r#type: t,
            priority: 3,
            group_id: Some(r.group_id),
            group_name: Some(r.group_name.clone()),
            title: title.to_string(),
            subtitle: Some(r.title.clone()),
            ref_id: Some(r.ref_id),
            action_url: format!("/pages/wishes/detail?id={}", r.ref_id),
        });
    }

    if unread_count > 0 {
        items.push(TodoItem {
            r#type: TodoType::UnreadNotifications,
            priority: 4,
            group_id: None,
            group_name: None,
            title: format!("{} 条未读消息", unread_count),
            subtitle: None,
            ref_id: None,
            action_url: "/pages/notifications/index".to_string(),
        });
    }

    items.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(a.group_id.cmp(&b.group_id))
            .then(a.ref_id.cmp(&b.ref_id))
    });

    let summary = TodayTodosSummary {
        sign_in_signed,
        sign_in_groups_pending: items
            .iter()
            .filter(|i| matches!(i.r#type, TodoType::SignIn))
            .count() as i32,
        orders_to_handle: order_rows.len() as i32,
        wishes_to_handle: wish_rows.len() as i32,
        unread_count: unread_count as i32,
    };

    Ok(ApiResponse::success(TodayTodosResponse {
        date: today_str,
        items,
        summary,
    }))
}

async fn fetch_sign_in(
    db: &sqlx::PgPool,
    user_id: i64,
    today: chrono::NaiveDate,
) -> Result<Vec<SignInRow>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT g.group_id, g.group_name,
               COALESCE(sr.sign_date IS NOT NULL, false) AS signed_today,
               COALESCE(sr.consecutive_days, 0) AS consecutive_days
        FROM association_group_members gm
        JOIN association_groups g ON g.group_id = gm.group_id
        LEFT JOIN sign_in_records sr
          ON sr.user_id = gm.user_id AND sr.group_id = gm.group_id AND sr.sign_date = $2
        WHERE gm.user_id = $1 AND gm.member_status = 'ACTIVE'::group_member_status_enum
        "#,
    )
    .bind(user_id)
    .bind(today)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SignInRow {
            group_id: r.get("group_id"),
            group_name: r.get("group_name"),
            signed_today: r.get("signed_today"),
            consecutive_days: r.get("consecutive_days"),
        })
        .collect())
}

async fn fetch_orders(db: &sqlx::PgPool, user_id: i64) -> Result<Vec<OrderRow>, sqlx::Error> {
    // 注意:orders 表的创建人列是 user_id(不是 creator_id)。
    let rows = sqlx::query(
        r#"
        SELECT 'ACCEPT' AS kind, o.order_id AS ref_id, o.group_id, g.group_name,
               o.title, o.created_at
        FROM orders o JOIN association_groups g ON g.group_id = o.group_id
        WHERE o.assignee_id = $1 AND o.status IN ('CREATED'::order_status_enum,'PENDING_ACCEPT'::order_status_enum)
        UNION ALL
        SELECT 'COMPLETE' AS kind, o.order_id, o.group_id, g.group_name, o.title, o.created_at
        FROM orders o JOIN association_groups g ON g.group_id = o.group_id
        WHERE o.assignee_id = $1 AND o.status IN ('ACCEPTED'::order_status_enum,'IN_PROGRESS'::order_status_enum)
        UNION ALL
        SELECT 'CONFIRM' AS kind, o.order_id, o.group_id, g.group_name, o.title, o.created_at
        FROM orders o JOIN association_groups g ON g.group_id = o.group_id
        WHERE o.user_id = $1
          AND o.status IN ('PRODUCTION_COMPLETED'::order_status_enum,'BREEDER_FINISHED'::order_status_enum)
        ORDER BY created_at ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| OrderRow {
            kind: r.get("kind"),
            ref_id: r.get("ref_id"),
            group_id: r.get("group_id"),
            group_name: r.get("group_name"),
            title: r.get("title"),
        })
        .collect())
}

async fn fetch_wishes(db: &sqlx::PgPool, user_id: i64) -> Result<Vec<WishRow>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT 'FULFILL' AS kind, w.wish_id AS ref_id, w.group_id, g.group_name,
               w.wish_name AS title, w.fulfillment_due_at AS sort_at
        FROM wishes w JOIN association_groups g ON g.group_id = w.group_id
        WHERE w.fulfiller_id = $1 AND w.status = 'CLAIMED'::wish_status_enum
        UNION ALL
        SELECT 'NEGOTIATE' AS kind, w.wish_id, w.group_id, g.group_name, w.wish_name, w.created_at
        FROM wishes w JOIN association_groups g ON g.group_id = w.group_id
        WHERE w.status = 'NEGOTIATING'::wish_status_enum
          AND (w.requester_id = $1 OR w.fulfiller_id = $1)
        ORDER BY sort_at ASC NULLS LAST
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| WishRow {
            kind: r.get("kind"),
            ref_id: r.get("ref_id"),
            group_id: r.get("group_id"),
            group_name: r.get("group_name"),
            title: r.get("title"),
        })
        .collect())
}

// ========== 纯函数辅助 (unit-testable) ==========

/// Map a kind discriminator from the SQL UNION query into a (TodoType, display title).
/// Pure function — easy to unit test.
fn order_kind_to_todo(kind: &str) -> (TodoType, &'static str) {
    match kind {
        "ACCEPT" => (TodoType::OrderAccept, "1 个订单待接单"),
        "COMPLETE" => (TodoType::OrderComplete, "1 个订单待完成"),
        "CONFIRM" => (TodoType::OrderConfirm, "1 个订单待确认"),
        _ => unreachable!("OrderRow.kind must be one of ACCEPT/COMPLETE/CONFIRM"),
    }
}

fn wish_kind_to_todo(kind: &str) -> (TodoType, &'static str) {
    match kind {
        "FULFILL" => (TodoType::WishFulfill, "1 个心愿待履约"),
        "NEGOTIATE" => (TodoType::WishNegotiate, "1 个心愿待协商"),
        _ => unreachable!("WishRow.kind must be one of FULFILL/NEGOTIATE"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_kind_mapping_covers_all_variants() {
        assert_eq!(order_kind_to_todo("ACCEPT").0, TodoType::OrderAccept);
        assert_eq!(order_kind_to_todo("COMPLETE").0, TodoType::OrderComplete);
        assert_eq!(order_kind_to_todo("CONFIRM").0, TodoType::OrderConfirm);
    }

    #[test]
    fn order_kind_titles_are_non_empty() {
        for kind in &["ACCEPT", "COMPLETE", "CONFIRM"] {
            let (_, title) = order_kind_to_todo(kind);
            assert!(!title.is_empty(), "title for {} must not be empty", kind);
        }
    }

    #[test]
    fn wish_kind_mapping_covers_all_variants() {
        assert_eq!(wish_kind_to_todo("FULFILL").0, TodoType::WishFulfill);
        assert_eq!(wish_kind_to_todo("NEGOTIATE").0, TodoType::WishNegotiate);
    }

    #[test]
    fn wish_kind_titles_are_non_empty() {
        for kind in &["FULFILL", "NEGOTIATE"] {
            let (_, title) = wish_kind_to_todo(kind);
            assert!(!title.is_empty(), "title for {} must not be empty", kind);
        }
    }

    #[test]
    fn todo_type_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&TodoType::OrderAccept).unwrap(),
            "\"order_accept\""
        );
        assert_eq!(
            serde_json::to_string(&TodoType::WishFulfill).unwrap(),
            "\"wish_fulfill\""
        );
        assert_eq!(
            serde_json::to_string(&TodoType::UnreadNotifications).unwrap(),
            "\"unread_notifications\""
        );
    }

    #[test]
    #[should_panic(expected = "must be one of")]
    fn order_kind_rejects_unknown_value() {
        order_kind_to_todo("UNKNOWN");
    }

    #[test]
    #[should_panic(expected = "must be one of")]
    fn wish_kind_rejects_unknown_value() {
        wish_kind_to_todo("UNKNOWN");
    }
}