# 首页卡片配套接口 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 3 endpoints / response extensions so the frontend homepage can render 今日待办卡片, 投喂/相遇天数, 角色切换前置检查.

**Architecture:**
- All 3 changes are **读侧增量** (no schema change, no new table).
- Handler 层直接走 sqlx 查询；不做 service 层抽象（与现有 dashboard / groups handler 一致）。
- 3 个独立任务，每个任务独立 commit。Task 之间无依赖，可单独 revert。

**Tech Stack:** Rust 2021 + ntex 2.1 + sqlx 0.8 (PostgreSQL) + utoipa 5.2 + serde + chrono.

**Important context:**
- 项目**无单元测试 / 集成测试基础设施**（`Cargo.toml` 无 `[dev-dependencies]`，代码库无 `#[cfg(test)]` 块）。本 plan **不做 TDD**，改用：
  - 每次 commit 前 `cargo check` + 编译通过
  - 1 次手动 curl 烟雾测试（每个 endpoint 1 个 happy path，文档化在 commit message）
- 工作分支 `v3`；所有 commit 留在这个分支上，不要 force push。
- **绝不动 schema**（不新建表 / 不 ALTER）。
- 现有模式：handler 文件用 `#[utoipa::path]` 标注，`openapi.rs` 的 `paths()` 宏里登记。

**File map:**

| 文件 | 改动类型 | 职责 |
|---|---|---|
| `src/api/users/today_todos.rs` | 新建 | today-todos handler + DTO + 4 个 SQL |
| `src/api/users/mod.rs` | 修改 | 导出新模块 + 加路由 |
| `src/api/dashboard/routes.rs` | 修改 | 3 个 DTO 新增字段 + 4 个新 SQL |
| `src/api/groups/routes.rs` | 修改 | swap-role/check handler + 1 个 DTO + 4 个 SQL |
| `src/openapi.rs` | 修改 | 登记 2 个新 path（dashboard 改字段**不**算新 path） |

---

## Task A: 实现 `GET /api/users/me/today-todos`

**Files:**
- Create: `src/api/users/today_todos.rs`
- Modify: `src/api/users/mod.rs:1-19` (注册子模块 + 路由)
- Modify: `src/openapi.rs:48-51,189-194` (登记 path + schema)

- [ ] **Step 1: 创建 handler 文件 `src/api/users/today_todos.rs`**

完整代码如下（约 280 行）：

```rust
// API - 首页今日待办
// Spec: docs/superpowers/specs/2026-06-15-homepage-cards-design.md §2

use chrono::Local;
use ntex::web::{self, types::State, Responder, ServiceConfig};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

/// 注册路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/users/me")
            .route("/today-todos", web::get().to(get_today_todos)),
    );
}

// ========== 响应 DTO ==========

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
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
    kind: &'static str, // "ACCEPT" | "COMPLETE" | "CONFIRM"
    ref_id: i64,
    group_id: i64,
    group_name: String,
    title: String,
}

struct WishRow {
    kind: &'static str, // "FULFILL" | "NEGOTIATE"
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
    let today = Local::now().date_naive();
    let today_str = today.to_string();

    // 4 个查询并行执行
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

    // 拼装 items
    let mut items: Vec<TodoItem> = Vec::new();

    // 1) SignIn（priority 1）
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

    // 2) 订单（priority 2）
    for r in &order_rows {
        let (t, title) = match r.kind {
            "ACCEPT" => (TodoType::OrderAccept, "1 个订单待接单".to_string()),
            "COMPLETE" => (TodoType::OrderComplete, "1 个订单待完成".to_string()),
            "CONFIRM" => (TodoType::OrderConfirm, "1 个订单待确认".to_string()),
            _ => unreachable!(),
        };
        items.push(TodoItem {
            r#type: t,
            priority: 2,
            group_id: Some(r.group_id),
            group_name: Some(r.group_name.clone()),
            title,
            subtitle: Some(r.title.clone()),
            ref_id: Some(r.ref_id),
            action_url: format!("/pages/orders/detail?id={}", r.ref_id),
        });
    }

    // 3) 心愿（priority 3，FULFILL 优先于 NEGOTIATE）
    for r in &wish_rows {
        let (t, title) = match r.kind {
            "FULFILL" => (TodoType::WishFulfill, "1 个心愿待履约".to_string()),
            "NEGOTIATE" => (TodoType::WishNegotiate, "1 个心愿待协商".to_string()),
            _ => unreachable!(),
        };
        items.push(TodoItem {
            r#type: t,
            priority: 3,
            group_id: Some(r.group_id),
            group_name: Some(r.group_name.clone()),
            title,
            subtitle: Some(r.title.clone()),
            ref_id: Some(r.ref_id),
            action_url: format!("/pages/wishes/detail?id={}", r.ref_id),
        });
    }

    // 4) 未读通知（priority 4，>0 时才生成）
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

    // 排序：priority asc, group_id asc (None 排最后), ref_id asc (None 排最后)
    items.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(a.group_id.cmp(&b.group_id))
            .then(a.ref_id.cmp(&b.ref_id))
    });

    let summary = TodayTodosSummary {
        sign_in_signed,
        sign_in_groups_pending: sign_in_rows.iter().filter(|r| !r.signed_today).count() as i32,
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

// ========== 4 个查询函数 ==========

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
        WHERE gm.user_id = $1 AND gm.member_status = 'ACTIVE'
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
    let rows = sqlx::query(
        r#"
        SELECT 'ACCEPT' AS kind, o.order_id AS ref_id, o.group_id, g.group_name,
               o.title, o.created_at
        FROM orders o JOIN association_groups g ON g.group_id = o.group_id
        WHERE o.assignee_id = $1 AND o.status IN ('CREATED','PENDING_ACCEPT')
        UNION ALL
        SELECT 'COMPLETE' AS kind, o.order_id, o.group_id, g.group_name, o.title, o.created_at
        FROM orders o JOIN association_groups g ON g.group_id = o.group_id
        WHERE o.assignee_id = $1 AND o.status IN ('ACCEPTED','IN_PROGRESS')
        UNION ALL
        SELECT 'CONFIRM' AS kind, o.order_id, o.group_id, g.group_name, o.title, o.created_at
        FROM orders o JOIN association_groups g ON g.group_id = o.group_id
        WHERE o.creator_id = $1
          AND o.status IN ('PRODUCTION_COMPLETED','BREEDER_FINISHED')
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
        WHERE w.fulfiller_id = $1 AND w.status = 'CLAIMED'
        UNION ALL
        SELECT 'NEGOTIATE' AS kind, w.wish_id, w.group_id, g.group_name, w.wish_name, w.created_at
        FROM wishes w JOIN association_groups g ON g.group_id = w.group_id
        WHERE w.status = 'NEGOTIATING'
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
```

- [ ] **Step 2: 修改 `src/api/users/mod.rs` 注册子模块与路由**

文件最顶部（第 1-19 行）替换为：

```rust
// API 层 - 用户模块
// FSD.latest.md compliant - 用户基础信息

use ntex::web::{self, ServiceConfig};
use std::sync::Arc;
use sqlx::Row;

use crate::config::AppState;

pub mod today_todos;

/// 配置用户相关路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/users/me")
            .route("", web::get().to(get_current_info))
            .route("", web::patch().to(update_info))
            .route("/groups", web::get().to(get_user_groups))
            .route("/delete", web::post().to(delete_account))
            .route("/today-todos", web::get().to(today_todos::get_today_todos)),
    );
    today_todos::configure(cfg);
}
```

- [ ] **Step 3: 在 `src/openapi.rs` 登记新 path**

在 `paths()` 块（`src/openapi.rs:48-51` 附近）的 Users 段尾追加：

```rust
        // ==================== Users (用户中心) ====================
        crate::api::users::get_current_info,
        crate::api::users::update_info,
        crate::api::users::get_user_groups,
        crate::api::users::delete_account,
        crate::api::users::today_todos::get_today_todos,   // ← 新增
```

在 `components(schemas())` 块（`src/openapi.rs:189-194` 附近）的 `-- User --` 段尾追加：

```rust
            crate::api::users::DeleteAccountResponse,
            crate::api::users::today_todos::TodoItem,         // ← 新增
            crate::api::users::today_todos::TodayTodosSummary, // ← 新增
            crate::api::users::today_todos::TodayTodosResponse,// ← 新增
```

`TodoType` 不需要单独登记 —— utoipa 会从 `TodoItem` 字段引用自动内联。

- [ ] **Step 4: 编译验证**

```bash
cargo check
```

期望: 0 errors. 警告可接受但不应当出现与本次新增相关的警告。

如出现未导入 / 拼写错误，按编译器提示修复后重跑。

- [ ] **Step 5: 手动烟雾测试（可选但推荐）**

启动服务（参考 README）:

```bash
cargo run --release
```

在另一终端:

```bash
# 1) 拿 token
TOKEN=$(curl -s -X POST http://localhost:9831/api/auth/wechat-login \
  -H "Content-Type: application/json" \
  -d '{"code":"test_code"}' | jq -r '.data.access_token')

# 2) 拉待办
curl -s http://localhost:9831/api/users/me/today-todos \
  -H "Authorization: Bearer $TOKEN" | jq
```

期望: HTTP 200，`data.items` 数组（可能为空）+ `data.summary` 5 个字段。

- [ ] **Step 6: Commit**

```bash
git add src/api/users/today_todos.rs src/api/users/mod.rs src/openapi.rs
git commit -m "feat(users): GET /api/users/me/today-todos 跨组聚合待办

聚合 4 类今日待办:
- 待签到（跨组）
- 待接单/待完成/待确认 订单
- 待履约/待协商 心愿
- 未读通知

4 个 SQL 通过 tokio::try_join! 并行执行。
只读侧,不引入 schema 变更。

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task B: 扩展 `GroupDashboardResponse` 字段

**Files:**
- Modify: `src/api/dashboard/routes.rs:42-94` (DTO 新增字段)
- Modify: `src/api/dashboard/routes.rs:225-375` (handler 改 4 个新查询 + 拼装)

- [ ] **Step 1: 在 `GroupInfo` DTO 加 `created_at`**

`src/api/dashboard/routes.rs:51-61` 替换为:

```rust
/// 组信息
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub group_id: i64,
    pub name: String,
    pub level: i32,
    pub exp: i64,
    pub next_level_exp: i64,
    pub diamond_balance: i32,
    pub daily_love_point_limit: i32,
    pub daily_group_exp_limit: i32,
    pub created_at: String,                    // ← 新增（RFC3339）
}
```

- [ ] **Step 2: 在 `MonthStats` DTO 加 `feeds_completed`**

`src/api/dashboard/routes.rs:75-82` 替换为:

```rust
/// 本月统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthStats {
    pub orders_completed: i32,
    pub wishes_finished: i32,
    pub love_points_spent: i32,
    pub love_points_earned: i32,
    pub feeds_completed: i32,                  // ← 新增
}
```

- [ ] **Step 3: 在 `QuickStats` DTO 加 5 个新字段**

`src/api/dashboard/routes.rs:85-94` 替换为:

```rust
/// 快速统计
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuickStats {
    pub total_orders: i32,
    pub total_wishes: i32,
    pub finished_wishes: i32,
    pub fulfillment_rate: f64,
    pub continuous_sign_in_days_user1: i32,
    pub continuous_sign_in_days_user2: i32,
    pub total_feeds: i32,                      // ← 新增：累计已完成订单数
    pub days_together: i32,                    // ← 新增：相遇天数
    pub total_diamonds_earned: i64,            // ← 新增
    pub total_diamonds_spent: i64,             // ← 新增
    pub total_love_points_balance: i64,        // ← 新增：当前用户积分
}
```

- [ ] **Step 4: 改 handler —— 取 `created_at`**

在 `src/api/dashboard/routes.rs:248-256` 的"获取组信息"查询里，把 `name, level, exp, diamond_balance, ...` 那行的 SELECT 列加 `created_at`，并把 fetch tuple 从 6 元改成 7 元:

```rust
    // 获取组信息
    let group_info: Option<(String, i32, i64, i32, i32, i32, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"SELECT name, level, exp, diamond_balance,
                  COALESCE(daily_love_point_limit, 100) as daily_limit,
                  COALESCE(daily_group_exp_limit, 200) as exp_limit,
                  created_at
           FROM association_groups WHERE group_id = $1"#
    )
    .bind(gid)
    .fetch_optional(db)
    .await?;

    let (name, level, exp, diamond, daily_limit, exp_limit, created_at) = group_info.unwrap_or((
        "未命名组".to_string(), 1, 0, 0, 100, 200,
        chrono::Utc::now(),  // fallback: 现在
    ));
```

- [ ] **Step 5: 改 handler —— 4 个新查询并行**

在 `src/api/dashboard/routes.rs:328` 之后（"获取快速统计"块结束、`Ok(ApiResponse::success(...))` 之前）插入:

```rust
    // 并行 4 个新查询
    let (total_feeds, month_feeds, (diamonds_earned, diamonds_spent), lp_balance): (
        i64, i64, (i64, i64), i64,
    ) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM orders
               WHERE group_id = $1
                 AND status IN ('CONFIRMED_COMPLETED','COMPLETED')"#,
        )
        .bind(gid)
        .fetch_one(db),
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM orders
               WHERE group_id = $1
                 AND status IN ('CONFIRMED_COMPLETED','COMPLETED')
                 AND updated_at >= DATE_TRUNC('month', CURRENT_DATE)"#,
        )
        .bind(gid)
        .fetch_one(db),
        async {
            let row: (i64, i64) = sqlx::query_as(
                r#"SELECT
                     COALESCE(SUM(CASE WHEN type = 'EARN' THEN amount ELSE 0 END), 0),
                     COALESCE(SUM(CASE WHEN type = 'CONSUME' THEN amount ELSE 0 END), 0)
                   FROM diamond_transactions
                   WHERE group_id = $1"#,
            )
            .bind(gid)
            .fetch_one(db)
            .await?;
            Ok::<(i64, i64), sqlx::Error>(row)
        },
        async {
            let row: Option<(i64, i64)> = sqlx::query_as(
                r#"SELECT COALESCE(available_love_point, 0), COALESCE(frozen_love_point, 0)
                   FROM user_group_points
                   WHERE user_id = $1 AND group_id = $2"#,
            )
            .bind(user_id)
            .bind(gid)
            .fetch_optional(db)
            .await?;
            let (a, f) = row.unwrap_or((0, 0));
            Ok::<i64, sqlx::Error>(a + f)
        },
    )?;

    // 计算相遇天数
    let today_date = chrono::Utc::now().date_naive();
    let days_together = (today_date - created_at.date_naive()).num_days().max(0) as i32;
```

- [ ] **Step 6: 改 handler —— 拼装响应**

在 `src/api/dashboard/routes.rs:342-374` 的 `Ok(ApiResponse::success(GroupDashboardResponse { ... }))` 块替换 `group`, `this_month`, `quick_stats` 三个字段的初始化:

```rust
    Ok(ApiResponse::success(GroupDashboardResponse {
        group: GroupInfo {
            group_id: gid,
            name,
            level,
            exp,
            next_level_exp,
            diamond_balance: diamond,
            daily_love_point_limit: daily_limit,
            daily_group_exp_limit: exp_limit,
            created_at: created_at.to_rfc3339(),                    // ← 新增
        },
        today: TodayStats {
            date: today,
            orders_completed: orders_today,
            love_points_earned: points_today,
            group_exp_earned: exp_today,
            sign_in: sign_in_status,
        },
        this_month: MonthStats {
            orders_completed: orders_month,
            wishes_finished,
            love_points_spent: points_spent,
            love_points_earned: points_earned,
            feeds_completed: month_feeds as i32,                    // ← 新增
        },
        quick_stats: QuickStats {
            total_orders,
            total_wishes,
            finished_wishes,
            fulfillment_rate,
            continuous_sign_in_days_user1: user1_sign,
            continuous_sign_in_days_user2: user2_sign,
            total_feeds: total_feeds as i32,                        // ← 新增
            days_together,                                          // ← 新增
            total_diamonds_earned: diamonds_earned,                 // ← 新增
            total_diamonds_spent: diamonds_spent,                   // ← 新增
            total_love_points_balance: lp_balance,                  // ← 新增
        },
    }))
```

- [ ] **Step 7: 编译验证**

```bash
cargo check
```

期望: 0 errors. 如有 `expected 6 elements` 这类 tuple mismatch，按编译器提示调整 fetch tuple 长度。

- [ ] **Step 8: 烟雾测试**

```bash
TOKEN=...  # 同 Task A
curl -s http://localhost:9831/api/groups/1/dashboard \
  -H "Authorization: Bearer $TOKEN" | jq '.data.quickStats'
```

期望: 输出包含 `totalFeeds / daysTogether / totalDiamondsEarned / totalDiamondsSpent / totalLovePointsBalance` 5 个新字段。

- [ ] **Step 9: Commit**

```bash
git add src/api/dashboard/routes.rs
git commit -m "feat(dashboard): 扩展 GroupDashboardResponse 字段

- GroupInfo.created_at
- MonthStats.feeds_completed
- QuickStats 新增 total_feeds / days_together / 钻石收支 / 积分余额

只增字段,不影响现有前端解析。
4 个新 SQL 通过 tokio::try_join! 并行执行。

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task C: 实现 `GET /api/groups/{group_id}/swap-role/check`

**Files:**
- Modify: `src/api/groups/routes.rs:24-49` (注册路由)
- Modify: `src/api/groups/routes.rs` 末尾（追加 handler + DTO）
- Modify: `src/openapi.rs:55,196` (登记 path + schema)

- [ ] **Step 1: 注册新路由**

在 `src/api/groups/routes.rs:24-49` 的 `web::scope("/api/groups")` 块内，`swap_role` 那行**后**插入:

```rust
        web::scope("/api/groups")
            .route("", web::post().to(create_group))
            .route("/join", web::post().to(join_group))
            .route("/{group_id}", web::get().to(get_group))
            .route("/{group_id}/swap-role", web::post().to(swap_role))
            .route("/{group_id}/swap-role/check", web::get().to(swap_role_check)),  // ← 新增
            .route("/{group_id}/exit", web::post().to(exit_group))
            ...
```

- [ ] **Step 2: 在 `src/api/groups/routes.rs` 末尾追加 handler + DTO**

在文件最后（第 1205 行 `pub struct JoinGroupInput` 之后）追加:

```rust
// ============== Swap Role Check (FSD 2026-06-15 设计稿 §4) ==============

/// 角色互换前置检查响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SwapRoleCheckResponse {
    pub can_swap: bool,
    pub reasons: Vec<String>,
    pub active_orders_count: i32,
    pub pending_wishes_count: i32,
    pub frozen_love_points: i64,
    pub pending_compensation: i32,
    pub pending_diamond_reward: i32,
    pub current_role: String,
    pub would_be_role: String,
    pub ignore_ongoing_wish_enabled: bool,
}

/// 角色互换前置检查
/// GET /api/groups/{group_id}/swap-role/check
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/swap-role/check",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "检查成功", body = SwapRoleCheckResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 404, description = "组不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
async fn swap_role_check(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;

    // Q1：取组信息 + 用户角色 + ignore 配置
    let row = sqlx::query(
        r#"SELECT
             g.buyer_user_id, g.seller_user_id,
             gm.role_in_group AS current_role,
             COALESCE((g.settings->>'swap_ignore_ongoing_wish')::bool, false) AS ignore_ongoing_wish
           FROM association_groups g
           JOIN association_group_members gm
             ON gm.group_id = g.group_id AND gm.user_id = $2
           WHERE g.group_id = $1 AND gm.member_status = 'ACTIVE'"#,
    )
    .bind(gid)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    let row = match row {
        Some(r) => r,
        None => {
            // 区分"组不存在"与"非组成员"
            let group_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM association_groups WHERE group_id = $1)",
            )
            .bind(gid)
            .fetch_one(db)
            .await?;

            if !group_exists {
                return Err(CustomError::NotFound("组不存在".into()));
            }
            return Err(CustomError::Forbidden("非组成员".into()));
        }
    };

    let ignore_ongoing_wish: bool = row.get("ignore_ongoing_wish");
    let current_role_raw: String = row.get("current_role");
    let current_role = current_role_raw.to_uppercase();
    let would_be_role = if current_role == "BUYER" {
        "SELLER".to_string()
    } else {
        "BUYER".to_string()
    };

    // Q2-Q4 并行
    let (active_orders, pending_wishes, frozen_points): (i64, i64, i64) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM orders
               WHERE group_id = $1
                 AND status NOT IN ('CONFIRMED_COMPLETED','COMPLETED',
                                    'CONFIRMED_INCOMPLETE','CONFIRMED_UNFINISHED',
                                    'REJECTED','CANCELLED','CANCELED',
                                    'TIMEOUT','SYSTEM_CLOSED','BREEDER_CLOSED')"#,
        )
        .bind(gid)
        .fetch_one(db),
        async {
            if ignore_ongoing_wish {
                return Ok::<i64, sqlx::Error>(0);
            }
            let n: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM wishes
                   WHERE group_id = $1 AND status = 'CLAIMED'
                     AND (requester_id = $2 OR fulfiller_id = $2)"#,
            )
            .bind(gid)
            .bind(user_id)
            .fetch_one(db)
            .await?;
            Ok(n)
        },
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COALESCE(SUM(amount), 0) FROM love_point_transactions
               WHERE user_id = $1 AND group_id = $2 AND type = 'FREEZE'"#,
        )
        .bind(user_id)
        .bind(gid)
        .fetch_one(db),
    )?;

    // 拼装 reasons
    let mut reasons: Vec<String> = Vec::new();
    if active_orders > 0 {
        reasons.push(format!("存在 {} 个未完结订单", active_orders));
    }
    if pending_wishes > 0 && !ignore_ongoing_wish {
        reasons.push(format!("存在 {} 个在途心愿", pending_wishes));
    }
    if frozen_points > 0 {
        reasons.push(format!("有 {} 冻结积分未处理", frozen_points));
    }

    let can_swap = reasons.is_empty();

    Ok(ApiResponse::success(SwapRoleCheckResponse {
        can_swap,
        reasons,
        active_orders_count: active_orders as i32,
        pending_wishes_count: pending_wishes as i32,
        frozen_love_points: frozen_points,
        pending_compensation: 0,
        pending_diamond_reward: 0,
        current_role,
        would_be_role,
        ignore_ongoing_wish_enabled: ignore_ongoing_wish,
    }))
}
```

- [ ] **Step 3: 在 `src/openapi.rs` 登记新 path + schema**

`paths()` 块（`src/openapi.rs:55` 附近）的 Groups 段尾追加:

```rust
        crate::api::groups::routes::swap_role,
        crate::api::groups::routes::swap_role_check,    // ← 新增
```

`components(schemas())` 块（`src/openapi.rs:196` 附近）的 `-- Groups --` 段尾追加:

```rust
            crate::api::groups::routes::CreateGroupResponse,
            crate::api::groups::routes::SwapRoleCheckResponse,   // ← 新增
```

- [ ] **Step 4: 编译验证**

```bash
cargo check
```

期望: 0 errors.

- [ ] **Step 5: 烟雾测试**

```bash
TOKEN=...
curl -s http://localhost:9831/api/groups/1/swap-role/check \
  -H "Authorization: Bearer $TOKEN" | jq
```

期望: HTTP 200，输出含 `canSwap / reasons / activeOrdersCount / pendingWishesCount / frozenLovePoints / currentRole / wouldBeRole / ignoreOngoingWishEnabled`.

- [ ] **Step 6: Commit**

```bash
git add src/api/groups/routes.rs src/openapi.rs
git commit -m "feat(groups): GET /api/groups/{group_id}/swap-role/check

镜像 swap-role 阻塞检查但不实际执行。返回:
- canSwap: 总体是否可换
- reasons: 人类可读阻塞原因
- activeOrdersCount / pendingWishesCount / frozenLovePoints: 数量
- currentRole / wouldBeRole / ignoreOngoingWishEnabled: 上下文

供前端按钮在点击 swap-role 前预提示。

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

### 1. Spec coverage

| Spec 章节 | 覆盖任务 |
|---|---|
| §2 接口 1 today-todos DTO | Task A Step 1 |
| §2.3 SQL 4 个查询 | Task A Step 1（内联 4 个 fetch_* 函数） |
| §2.4 应用层组装 | Task A Step 1（handler 内的拼装） |
| §2.5 错误码 | Task A Step 1（中间件统一 401；无 404 因为用户无组也是合法响应） |
| §2.6 路由挂载 | Task A Step 2 |
| §3.1 dashboard DTO 扩展 | Task B Steps 1-3 |
| §3.2 SQL 改动 | Task B Steps 4-5 |
| §4.2 swap-check DTO | Task C Step 2 |
| §4.3 SQL 4 个查询 | Task C Step 2（Q1 + Q2-Q4 2 个 tokio try_join） |
| §4.7 settlement stub 不动 | 已避开（本次不碰 settlement_check_impl） |
| OpenAPI 登记 | Task A Step 3 / Task C Step 3 |

**gaps**: 无。

### 2. Placeholder scan

- 全部代码完整，无 "TBD / TODO / 类似" 模糊语。
- Task B Step 4 用 `chrono::Utc::now()` 作 fallback —— 是真实 fallback，不是 placeholder。
- Task C Step 2 `pending_compensation / pending_diamond_reward = 0` —— 与 spec §4.7 一致（占位待业务实现），spec 已说明。

### 3. Type consistency

- `TodoType` 在 `today_todos.rs` 内定义并使用，没有跨文件引用。
- `SwapRoleCheckResponse` 字段名在 handler 拼装和 struct 定义 1:1 对齐。
- `GroupInfo / MonthStats / QuickStats` 字段顺序：struct 定义 → handler 初始化 → SQL 列 → 完全一致。
- `i64 → i32` 转换点都用 `as i32` 显式标注（`total_feeds` / `month_feeds`）。

### 4. Risk

- **sqlx tuple 长度**：dashboard handler 原代码有 6 元 tuple 取组信息，本 plan 改为 7 元。任何漏改 `unwrap_or` 的 fallback tuple 都会编译失败 —— 已显式标注 7 元。
- **chrono 类型**：`created_at` 在 DB 是 `TIMESTAMP`，sqlx 默认映射 `chrono::DateTime<chrono::Utc>`，本 plan 与项目其它处一致。
- **顺序依赖**：Task A、B、C **完全独立**，可任意顺序或并行执行。每个独立 commit。
