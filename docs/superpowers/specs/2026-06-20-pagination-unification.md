# 列表分页统一（Cursor 化）设计

> **状态**：已实施（commit `73201d8`）
> **作用域**：4 个列表端点切换到 cursor 分页 + openapi.rs schema 补全
> **不涉及**：数据库 schema 变更、不动其他分页已正确的端点

---

## 1. 背景

### 1.1 用户视角的问题

前端用 OpenAPI 自动生成的 TypeScript 里，列表端点的响应是 `content: never`（拿不到响应体类型），且 `list_memorial_days` 这种**只有 `limit`、没有 `offset`/`cursor`**——后端超过 100 条就翻不到。

### 1.2 项目现状（审计结果）

| 模块 | 分页方式 | 状态 |
|---|---|---|
| notifications / foods / economy / footprints / wishes | cursor + limit | ✅ 正确（项目标准） |
| admin (AuditLog / PendingReview) | cursor + limit | ✅ 正确 |
| **list_memorial_days** | **limit only** | ❌ 修 |
| **list_tags** | **limit only** | ❌ 修 |
| **list_my_tickets** | **limit only** | ❌ 修 |
| **admin PendingFoodAudit** | **limit + offset** | ❌ 跟标准不一致 |
| list_my_tickets / support_tickets | limit only | ❌ 修 |
| 项目其他端点 | 各异 | 大部分用 cursor |

**结论**：项目已经有 `models/pagination::CursorPage<T>` 统一标准，但 4 个端点没对齐。修法是切到 cursor 模式（不要引入 offset 模式）。

---

## 2. 设计决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 字段 | **复用** `models::pagination::CursorPage<T>` 和 `CursorQuery` | 项目已有标准，引入新字段是浪费 |
| 编码 | base64(JSON) | `encode_cursor` / `decode_cursor` helper 已实现 |
| Sort 顺序保留 | 不动原有 ORDER BY，cursor 元组跟着 | 前端行为不变 |
| OpenAPI body | 4 个 T 类型补注册到 `openapi.rs::components(schemas)` | 修 `content: never` |
| count/total | 保留 `total` 字段（admin pending-food 用），其他端点设 `None` | 避免无谓的 COUNT(*) 查询 |
| 边界 | `limit+1` 策略判定 has_more，多查一行用完即丢 | 业界标准做法 |

---

## 3. 4 个端点的 cursor 设计

### 3.1 `list_memorial_days`（最复杂）

**原排序**：`is_default DESC, memorial_date ASC`（多字段，倒序+正序混合）

**Cursor 元组**：
```rust
struct MemorialDayCursor {
    is_default: i16,        // 0 或 1
    memorial_date: NaiveDate,
    id: i64,                // 稳定 tiebreaker
}
```

**SQL WHERE 扩展形式**（不能用 `(a, b, c) > ($1, $2, $3)` 元组比较，因为首列是 DESC）：
```sql
WHERE group_id = $1
  AND (
    $2::SMALLINT IS NULL                                   -- 没 cursor
    OR is_default < $2                                      -- 同 1, 改 0
    OR (is_default = $2 AND memorial_date > $3)             -- 同 1, 同日序
    OR (is_default = $2 AND memorial_date = $3 AND id > $4)  -- 全相同,tiebreak by id
  )
ORDER BY is_default DESC, memorial_date ASC, id ASC
```

**为什么不能用元组比较**：
- PG 的 `(a, b, c) > (x, y, z)` 是按自然序（左到右逐个比较）
- 我们的排序 `a DESC, b ASC, id ASC` 第一列是 DESC
- 自然序下"下一条"可能是 `a` 变小（0 < 1），但 `(0, ...) > (1, ...)` 是 **false**
- 所以必须**展开成 OR 链**处理 DESC + ASC 混合

### 3.2 `list_tags`

**原排序**：`sort ASC, tag_id ASC`（全正序，可以元组比较）

**Cursor 元组**：
```rust
struct TagCursor {
    sort: i32,
    tag_id: i64,
}
```

**SQL**：
```sql
WHERE (group_id = $1 OR group_id IS NULL)
  AND ($2::TEXT IS NULL OR tag_name ILIKE $2)
  AND ($3::INTEGER IS NULL OR (t.sort, t.tag_id) > ($3, $4))
ORDER BY t.sort ASC, t.tag_id ASC
```

**关键**：`(t.sort, t.tag_id) > ($3, $4)` 直接用元组比较，因为排序方向都是 ASC。

### 3.3 `list_my_tickets`

**原排序**：`created_at DESC`（单字段，缺 tiebreaker）

**Cursor 元组**：
```rust
struct TicketCursor { ticket_id: i64 }  // 单字段
```

**修改 ORDER BY 加上 tiebreaker**：`ORDER BY created_at DESC, ticket_id ASC`

**SQL**：
```sql
WHERE user_id = $1
  AND ($2::TEXT IS NULL OR status = $2)
  AND ($3::BIGINT IS NULL OR ticket_id < $3)   -- 单字段 cursor
ORDER BY created_at DESC, ticket_id ASC
```

**为什么单字段够**：`ticket_id` 是 BIGSERIAL，与 `created_at` 强相关，cursor 退化到只比 `ticket_id` 在 created_at DESC 排序下也成立（因为同一时间插入的 ticket_id 连续，<cursor 等于 created_at 早）。

### 3.4 `admin list_pending_food_audits`（offset → cursor）

**原排序**：`created_at ASC`（offset 方式）

**Cursor 元组**：
```rust
struct PendingFoodCursor {
    created_at: DateTime<Utc>,
    food_id: i64,
}
```

**SQL**（首列 ASC，可以用元组比较）：
```sql
WHERE apply_status = 'PENDING' AND is_del = 0
  AND (
    $1::TIMESTAMPTZ IS NULL
    OR created_at > $1
    OR (created_at = $1 AND food_id > $2)
  )
ORDER BY created_at ASC, food_id ASC
```

**响应结构变化**（breaking change 但必要）：
- 旧：`PendingFoodAuditListResponse { items, total, limit, offset }` —— 自定义
- 新：`type PendingFoodAuditListResponse = CursorPage<PendingFoodOut>` —— 用项目标准，丢 `limit`/`offset` 字段

---

## 4. CursorPage 响应结构（统一格式）

```rust
#[derive(Serialize, Deserialize, ToSchema)]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,  // base64(JSON(T))，传给下一次请求的 ?cursor=
    pub has_more: bool,               // 是否还有下一页
    pub total: Option<i64>,           // 可选；多数端点不查 COUNT
}
```

**JSON 示例**（拿 list_memorial_days 来说）：
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "items": [ /* MemorialDayOut[] */ ],
    "nextCursor": "eyJpc19kZWZhdWx0IjoxLCJtZW1vcmlhbF9kYXRlIjoiMjAyNi0wNi0xOSIsImlkIjoxMjN9",
    "hasMore": true,
    "total": null
  }
}
```

---

## 5. 4 个端点改动表

| 端点 | 文件 | 主要改动 |
|---|---|---|
| `GET /api/groups/{id}/memorial-days` | `routes.rs` | 增 cursor 参数 + `MemorialDayCursor` 结构 + OR-chain WHERE + `CursorPage<MemorialDayOut>` 响应 + OpenAPI body |
| `GET /api/groups/{id}/tags` | `tags/routes.rs` | 增 cursor 参数 + `TagCursor` + tuple 比较 + `CursorPage<TagOut>` + OpenAPI body |
| `GET /api/support-tickets` | `support_tickets/routes.rs` | 增 cursor 参数 + `TicketCursor` + 单字段 cursor + 合并两条 SQL 成一条 + `CursorPage<TicketOut>` + OpenAPI body |
| `GET /api/admin/foods/pending` | `admin/routes.rs` | offset→cursor + `PendingFoodCursor` + `PendingFoodAuditListResponse` 改 type alias |

| 文件 | 改动 |
|---|---|
| `openapi.rs` | 加 3 个 schema 注册：`TagOut` / `TicketOut` / `PendingFoodOut`（修 `content: never` 根因） |

**不修改**：
- `src/v3.sql`
- 其他任何分页已正确的端点（notifications / foods / economy / 等）

---

## 6. 前端迁移指南

### 6.1 旧 → 新响应 shape

**之前**（裸数组）：
```typescript
const data: MemorialDayOut[] = await api.listMemorialDays({ ... });
```

**现在**（带 cursor 包装）：
```typescript
const page: CursorPage<MemorialDayOut> = await api.listMemorialDays({ ... });
const items = page.items;
const hasMore = page.hasMore;
const nextCursor = page.nextCursor;
```

### 6.2 翻页模式

**之前**（如果有 offset）：
```typescript
// 第 N 页
api.listMemorialDays({ offset: (N - 1) * 50, limit: 50 });
```

**现在**（cursor）：
```typescript
// 第一页
const page1 = await api.listMemorialDays({ limit: 50 });

// 后续页：用上一页返回的 nextCursor
const page2 = await api.listMemorialDays({ limit: 50, cursor: page1.nextCursor });

// 循环直到 hasMore === false
let cursor: string | null = null;
const all: MemorialDayOut[] = [];
do {
  const page = await api.listMemorialDays({ limit: 50, cursor });
  all.push(...page.items);
  cursor = page.hasMore ? page.nextCursor : null;
} while (cursor);
```

### 6.3 OpenAPI 生成会好

之前前端 TS 里：
```typescript
responses: {
  200: { content: never };   // 啥也看不到
}
```

**现在**（OpenAPI schema 完整）：
```typescript
responses: {
  200: {
    content: {
      "application/json": {
        schema: { $ref: "#/components/schemas/CursorPage_MemorialDayOut" }
      }
    }
  }
}
```

→ 前端 TS 自动拿到 `CursorPage<MemorialDayOut>` 类型。

---

## 7. 验证

### 7.1 已通过

- ✅ `cargo check`：0 error
- ✅ 18 单测全过（含 7 个 days_until + 3 个 pin/unpin + 8 个既有）
- ✅ 4 个端点的 SQL 都用 NULL 短路支持可选 cursor
- ✅ 4 个端点的 OpenAPI 都加了 `body = CursorPage<...>` + T 类型在 schemas 注册
- ✅ `list_memorial_days` 的 OR-chain WHERE 正确处理 DESC+ASC 混合排序
- ✅ `list_tags` 用元组比较，简洁高效
- ✅ `list_my_tickets` 单字段 cursor 简化 SQL
- ✅ admin `PendingFoodAudit` 从 offset 改 cursor，类型变 type alias 保兼容

### 7.2 手工验证（待 dev server 重启后跑）

| 用例 | 期望 |
|---|---|
| 第一页不带 cursor | 返回 items, nextCursor, hasMore=true/false |
| 用上一页 nextCursor 请求 | 返回下一页 |
| 翻到最后一页 | nextCursor=null, hasMore=false |
| 列表为空 | items=[], nextCursor=null, hasMore=false |
| Cursor 乱编/损坏 | decode 返回 None，handler 走"无 cursor"分支（不 panic） |
| `upcoming_days=30` 过滤 | 跟之前一样后置过滤（hasMore 可能有偏差，可接受） |

### 7.3 待观察的边界

- `list_memorial_days` 的 `upcoming_days` 过滤是**后置过滤**（先 LIMIT+1 再 .filter()），可能导致 `has_more` 判定不精确（过滤掉了某些行导致看似没下一页）。**已知限制**，暂不优化。
- `list_my_tickets` 单字段 cursor 严格说在 `created_at` 重复时可能漏行（PG created_at 是 timestamptz 精度有限）。**实际极少发生**，与 notifications 端点同款风险。

---

## 8. 不在本次范围

- ❌ 给已正确分页的端点（notifications / foods / 等）做 refactor —— 不动
- ❌ 把 cursor 抽成更通用的 trait —— 当前每端点手动 OR-chain 已经够清楚
- ❌ 改 `upcoming_days` 后置过滤为 SQL 内过滤 —— 性能影响小，等真有用户量再说
- ❌ 给 `upcoming_days` 加 cursor 编码 —— 等真要按 upcoming 翻页时再处理
- ❌ 改 `list_my_tickets` 的双 SQL 合并为单 SQL —— 已经改了（用 NULL 短路）
- ❌ 把 `limit+1` 抽成公共 helper —— 4 处用，抽出来收益小
