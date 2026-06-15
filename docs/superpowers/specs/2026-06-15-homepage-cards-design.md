# 前端首页卡片配套接口设计

> **状态**：已批准（用户口头同意 2026-06-15）
> **作用域**：首页 3 张卡片的 3 个新接口 / 1 个响应扩展
> **不涉及**：schema 变更（不新建表），仅 SQL 查询 + DTO + 路由

---

## 1. 背景与目标

前端首页（Homepage）需展示：
1. **今日待办卡片** —— 跨组聚合的"我今天要做什么"
2. **组内统计** —— 累计投喂次数、相遇天数等
3. **纪念日卡片** —— ✅ 已有 `memorial-days` 接口，本次不动
4. **角色切换按钮** —— 需要前置"是否可切换"判断（含活跃订单数）

现状缺口（见 `src/api/` 全量扫描）：
- ❌ 无 `/api/users/me/today-todos` —— 4 类待办分散在 4 个模块
- ❌ `GroupDashboardResponse` 缺少 `total_feeds` / `days_together` / `total_diamonds_earned` / `total_love_points_balance` 等
- ❌ 无 `/api/groups/{group_id}/swap-role/check` —— `swap-role` 动作接口失败时只返回 400 + 原因文字，**不返回阻塞数量**

本设计仅做"读侧"扩展，不引入新表，不动 schema。

---

## 2. 接口 1：今日待办聚合

### 2.1 端点

```
GET /api/users/me/today-todos
```

- Tag: **用户**（与 `get_current_info` 同 tag）
- Auth: `bearer_auth`
- 路径参数：无
- Query 参数：无（**v1 不传 group_id**，跨组聚合）

### 2.2 响应 DTO

```rust
// src/api/users/today_todos.rs

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodayTodosResponse {
    pub date: String,                    // YYYY-MM-DD（服务端当日）
    pub items: Vec<TodoItem>,            // 已按 priority 升序
    pub summary: TodayTodosSummary,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub r#type: TodoType,                // SIGN_IN / ORDER_ACCEPT / ORDER_COMPLETE / ORDER_CONFIRM / WISH_FULFILL / WISH_NEGOTIATE / UNREAD_NOTIFICATIONS
    pub priority: u8,                    // 1=最高
    pub group_id: Option<i64>,           // SIGN_IN 时必填；UNREAD 时为 None
    pub group_name: Option<String>,
    pub title: String,                   // "今日签到" / "1 个订单待接单"
    pub subtitle: Option<String>,        // "已连续签到 7 天" / 订单标题
    pub ref_id: Option<i64>,             // 跳转用：order_id / wish_id
    pub action_url: String,              // 前端路由模板
}

#[derive(Debug, Serialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
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
pub struct TodayTodosSummary {
    pub sign_in_signed: bool,            // 用户是否已在**任一组**今日签到（任一组签了即 true，组内全签概念由 sign-in/status 处理）
    pub sign_in_groups_pending: i32,     // 还没签的组数
    pub orders_to_handle: i32,           // 待接单 + 待完成 + 待确认 之和
    pub wishes_to_handle: i32,
    pub unread_count: i32,
}
```

### 2.3 SQL 查询（3 次 round-trip）

按下列顺序在事务外并行执行（`tokio::join!`），全部在 1 个 handler 内串行 await：

**Q1 — 待签到（跨组）**：
```sql
SELECT g.group_id, g.group_name,
       COALESCE(sr.sign_date IS NOT NULL, false) AS signed_today,
       COALESCE(sr.consecutive_days, 0) AS consecutive_days
FROM association_group_members gm
JOIN association_groups g ON g.group_id = gm.group_id
LEFT JOIN sign_in_records sr
  ON sr.user_id = gm.user_id AND sr.group_id = gm.group_id AND sr.sign_date = CURRENT_DATE
WHERE gm.user_id = $1 AND gm.member_status = 'ACTIVE'
```

应用层过滤 `signed_today = false` 产 TodoItem(type=SignIn)。

**Q2 — 待处理订单（4 类 union）**：
```sql
-- 待接单
SELECT 'ACCEPT' AS kind, o.order_id AS ref_id, o.group_id, g.group_name,
       o.title, o.created_at
FROM orders o JOIN association_groups g ON g.group_id = o.group_id
WHERE o.assignee_id = $1 AND o.status IN ('CREATED','PENDING_ACCEPT')
UNION ALL
-- 待完成
SELECT 'COMPLETE' AS kind, o.order_id, o.group_id, g.group_name, o.title, o.created_at
FROM orders o JOIN association_groups g ON g.group_id = o.group_id
WHERE o.assignee_id = $1 AND o.status IN ('ACCEPTED','IN_PROGRESS')
UNION ALL
-- 待确认（buyer 侧）
SELECT 'CONFIRM' AS kind, o.order_id, o.group_id, g.group_name, o.title, o.created_at
FROM orders o JOIN association_groups g ON g.group_id = o.group_id
WHERE o.creator_id = $1
  AND o.status IN ('PRODUCTION_COMPLETED','BREEDER_FINISHED')
ORDER BY created_at ASC
```

> 注：不做 `assignee_id` 排除 creator_id 的反向检查，因为 `orders` 表里 creator 与 assignee 在 NORMAL 订单里天然不同。

**Q3 — 待处理心愿（2 类 union）**：
```sql
-- 待履约（fulfiller 侧 CLAIMED）
SELECT 'FULFILL' AS kind, w.wish_id AS ref_id, w.group_id, g.group_name,
       w.wish_name AS title, w.fulfillment_due_at AS sort_at
FROM wishes w JOIN association_groups g ON g.group_id = w.group_id
WHERE w.fulfiller_id = $1 AND w.status = 'CLAIMED'
UNION ALL
-- 待协商（requester 或 fulfiller 侧 NEGOTIATING）
SELECT 'NEGOTIATE' AS kind, w.wish_id, w.group_id, g.group_name, w.wish_name, w.created_at
FROM wishes w JOIN association_groups g ON g.group_id = w.group_id
WHERE w.status = 'NEGOTIATING'
  AND (w.requester_id = $1 OR w.fulfiller_id = $1)
ORDER BY sort_at ASC NULLS LAST
```

**Q4 — 未读通知总数**：
```sql
SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false
```

> 决定**不查未读 list**（避免一次首页拉几百条），只返回 count + 1 条 `UnreadNotifications` TodoItem。

### 2.4 应用层组装顺序

1. await 4 个 SQL
2. 把 Q1 未签到的组生成 SignIn TodoItem（priority=1）
3. Q2 每一行按 `kind` 映射到对应 enum（priority=2）
4. Q3 每一行按 `kind` 映射（priority=3，CLAIMED 排在 NEGOTIATING 前）
5. Q4 > 0 时生成 1 条 UnreadNotifications TodoItem（priority=4）
6. **合并并按 `(priority asc, group_id asc, ref_id asc)` 排序**
7. 计算 `summary` 各 count
8. 序列化

### 2.5 错误

| 状态码 | 触发条件 |
|---|---|
| 401 | token 失效（中间件） |
| 500 | DB 异常 |

不查"用户是否在组"——**不在任何组**也是合法响应（`items=[]`）。

### 2.6 路由挂载

改 `src/api/users/mod.rs::configure`：
```rust
web::scope("/api/users/me")
    .route("", web::get().to(get_current_info))
    .route("", web::patch().to(update_info))
    .route("/groups", web::get().to(get_user_groups))
    .route("/delete", web::post().to(delete_account))
    .route("/today-todos", web::get().to(get_today_todos)),   // ← 新增
```

`get_today_todos` 实现放在新文件 `src/api/users/today_todos.rs`，`mod.rs` 加 `pub mod today_todos;` 并 `pub use today_todos::get_today_todos;`。

### 2.7 测试

`src/api/users/today_todos.rs` 文件底部 `#[cfg(test)] mod tests`：
- happy path：构造 1 个 ACTIVE 组、1 个未签到、1 个待接单订单、1 个 CLAIMED 心愿、3 条未读通知 → 期望 5 条 items + 正确 summary
- 边界：用户无组 → items 空数组（**不**返回 404）
- 边界：所有任务都已完成 → items 只有 summary 全 0

数据库测试需要 mock，本次**不**接入 sqlx::test（项目目前无此基础设施），改用编译期检查 + 单元测试 enum 映射正确性。

---

## 3. 接口 2：扩展 dashboard 响应

### 3.1 改动范围

**仅** `src/api/dashboard/routes.rs`，**只增字段**（前端向后兼容）：

| 结构 | 现有字段 | 新增字段 | 类型 | 含义 / 来源 |
|---|---|---|---|---|
| `GroupInfo` | 8 个 | `created_at: String` | RFC3339 | `association_groups.created_at` |
| `MonthStats` | 4 个 | `feeds_completed: i32` | i32 | 本月 CONFIRMED_COMPLETED 订单数 |
| `QuickStats` | 6 个 | `total_feeds: i32` | i32 | 累计 CONFIRMED_COMPLETED 订单数（**组内全部**） |
| `QuickStats` | 6 个 | `days_together: i32` | i32 | `(today - group.created_at).num_days()`，**服务端算** |
| `QuickStats` | 6 个 | `total_diamonds_earned: i64` | i64 | `SUM(amount) WHERE group_id=$gid AND type='EARN'`，0 if NULL |
| `QuickStats` | 6 个 | `total_diamonds_spent: i64` | i64 | `SUM(amount) WHERE group_id=$gid AND type='CONSUME'`，0 if NULL |
| `QuickStats` | 6 个 | `total_love_points_balance: i64` | i64 | 当前用户在该组的 `available_love_point + frozen_love_point`（来自 `user_group_points`） |

> **投喂口径**：用户已确认 = `orders.status = 'CONFIRMED_COMPLETED'` 的订单数（**全组累计**）。

### 3.2 SQL 改动

把现有 dashboard 的 3 个 sub-query 中"获取组信息"那段扩成一次多列查询，并在 `QuickStats` 拼装前并行 3 个新查询：

```sql
-- 新 Q：累计投喂（替代 total_orders 的子集）
SELECT COUNT(*) FROM orders
WHERE group_id = $1 AND status IN ('CONFIRMED_COMPLETED', 'COMPLETED');

-- 新 Q：本月投喂
SELECT COUNT(*) FROM orders
WHERE group_id = $1
  AND status IN ('CONFIRMED_COMPLETED', 'COMPLETED')
  AND updated_at >= DATE_TRUNC('month', CURRENT_DATE);

-- 新 Q：累计钻石
SELECT
  COALESCE(SUM(CASE WHEN type = 'EARN' THEN amount ELSE 0 END), 0),
  COALESCE(SUM(CASE WHEN type = 'CONSUME' THEN amount ELSE 0 END), 0)
FROM diamond_transactions
WHERE group_id = $1;

-- 新 Q：用户在该组当前积分
SELECT COALESCE(available_love_point, 0) + COALESCE(frozen_love_point, 0)
FROM user_group_points
WHERE user_id = $1 AND group_id = $2;
```

**性能取舍**：原 dashboard 已经是 3 次 round-trip，再加 4 次 = 7 次。**改用 `tokio::join!` 并行 4 次新查询**，端到端延迟 = max(4 queries) ≈ 8-15ms，可接受。

### 3.3 错误

不变（同现有 dashboard handler）。

---

## 4. 接口 3：swap-role 前置检查

### 4.1 端点

```
GET /api/groups/{group_id}/swap-role/check
```

- Tag: **双人组**（与 `swap_role` 同 tag）
- Auth: `bearer_auth`
- 路径参数：`group_id`
- Query 参数：无

### 4.2 响应 DTO

```rust
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SwapRoleCheckResponse {
    pub can_swap: bool,
    pub reasons: Vec<String>,            // 人类可读，"存在 N 个未完结订单" / "存在 N 个在途心愿" / "有 N 冻结积分未处理"
    pub active_orders_count: i32,
    pub pending_wishes_count: i32,
    pub frozen_love_points: i64,
    pub pending_compensation: i32,       // 占位 0（订单补偿业务未实现，参见 settlement_check 现有 stub）
    pub pending_diamond_reward: i32,     // 占位 0
    pub current_role: String,            // "BUYER" / "SELLER"
    pub would_be_role: String,           // 镜像反向
    pub ignore_ongoing_wish_enabled: bool,
}
```

### 4.3 SQL（**1 个连接查询 + 3 个并行 COUNT**）

```sql
-- Q1：取组配置 + 用户角色 + 即将对调的角色
SELECT
  g.buyer_user_id, g.seller_user_id,
  gm.role_in_group AS current_role,
  COALESCE((g.settings->>'swap_ignore_ongoing_wish')::bool, false) AS ignore_ongoing_wish
FROM association_groups g
JOIN association_group_members gm
  ON gm.group_id = g.group_id AND gm.user_id = $2
WHERE g.group_id = $1 AND gm.member_status = 'ACTIVE';

-- Q2（并行）：未完结在途订单
SELECT COUNT(*) FROM orders
WHERE group_id = $1
  AND status NOT IN ('CONFIRMED_COMPLETED','COMPLETED','CONFIRMED_INCOMPLETE','CONFIRMED_UNFINISHED',
                     'REJECTED','CANCELLED','CANCELED','TIMEOUT','SYSTEM_CLOSED','BREEDER_CLOSED');

-- Q3（并行）：当前用户的 CLAIMED 在途心愿
SELECT COUNT(*) FROM wishes
WHERE group_id = $1 AND status = 'CLAIMED'
  AND (requester_id = $2 OR fulfiller_id = $2)
  AND $ignore_ongoing_wish = false;       -- 若 ignore=true 则强制返回 0

-- Q4（并行）：用户在该组的冻结积分
SELECT COALESCE(SUM(amount), 0) FROM love_point_transactions
WHERE user_id = $2 AND group_id = $1 AND type = 'FREEZE';
```

### 4.4 应用层组装

- `current_role` 转大写
- `would_be_role` = if BUYER → "SELLER" else "BUYER"
- `reasons`：根据 count > 0 依次 push
  - `active_orders_count > 0` → `"存在 {N} 个未完结订单"`
  - `pending_wishes_count > 0 && !ignore_ongoing_wish` → `"存在 {N} 个在途心愿"`
  - `frozen_love_points > 0` → `"有 {N} 冻结积分未处理"`
- `can_swap = reasons.is_empty()`

### 4.5 错误

| 状态码 | 触发条件 |
|---|---|
| 401 | token 失效 |
| 403 | 非组成员（Q1 返回 0 行） |
| 404 | 组不存在（Q1 查不到 group） |
| 500 | DB 异常 |

### 4.6 路由挂载

改 `src/api/groups/routes.rs::configure`：
```rust
.service(
    web::scope("/api/groups")
        ...
        .route("/{group_id}/swap-role", web::post().to(swap_role))
        .route("/{group_id}/swap-role/check", web::get().to(swap_role_check)),   // ← 新增
        ...
);
```

### 4.7 顺带修复（**scope creep 提示，不在本 PR**）

`groups/routes.rs:442` 的 `settlement_check` 内部 `pending_orders: 0` 是写死 0 的 stub。**本期不动它**（避免越界），仅在 spec 记录，后续单独 PR。

---

## 5. OpenAPI 注册

`src/openapi.rs` 需要追加 3 个 `#[utoipa::path]`（实际上 utoipa 通过 `#[utoipa::path]` attribute 自动收集，handler 文件内已加），但**模块需要在 `#[openapi_paths]` 宏里登记**。

待 implementation 时检查并补。

---

## 6. 验收清单

- [ ] 接口 1：3 个 SQL 文件可独立读懂；返回示例与 §2.2 一致
- [ ] 接口 2：原有 dashboard 字段保持不变；新增字段在 401/500 路径下行为不变
- [ ] 接口 3：与 `swap-role`（POST）行为对齐 —— 任何 `swap-role` 拒绝的场景，本接口都返回 `canSwap=false` + 对应 `reasons`
- [ ] 不修改 schema（不新建表 / 不 ALTER）
- [ ] 不修改任何现有 handler 的 SQL（接口 2 在 dashboard handler 内部追加 4 个新查询，不动原 3 个查询）
- [ ] `cargo check` 通过
- [ ] `cargo build --release` 通过
- [ ] 没有引入新的 crate 依赖

---

## 7. 不在本次范围

- 任何写操作（不创建待办、完成待办等）
- 任何 schema 变更
- `settlement_check` 内部 pending_orders 写死 0 的 bug 修复（独立 PR）
- 缓存（看板类不强一致，按 FSD §12 要求"进入页面强制刷新"）
- 多语言（reason 文案暂只提供中文，与现有 dashboard 文案风格一致）
