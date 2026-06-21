# 纪念日置顶（is_default）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让用户可以在一组纪念日里挑 1 条置顶，置顶的会在列表最前面、首页纪念日卡片直接展示该条；切换置顶自动清掉旧的。

**Architecture:** 复用现有 `is_default` 字段（不改 schema），加 2 个独立端点 `POST/DELETE /pin` 做置顶/取消置顶，事务保证"先清后设"的原子性；列表查询加 `is_default DESC` 排序让置顶的永远在最前。

**Tech Stack:** Rust 1.x, sqlx 0.7, ntex 2.1, utoipa 4.x, PostgreSQL 15

**Spec:** [`docs/superpowers/specs/2026-06-19-memorial-pin-design.md`](../specs/2026-06-19-memorial-pin-design.md)

---

## File Structure

| 文件 | 改动 | 职责 |
|---|---|---|
| `src/api/memorial_days/routes.rs` | 修改 | 加 2 handler、加路由、改 2 个查询的 ORDER BY、加 DTO 类型、加单测 |
| `src/openapi.rs` | 修改 | 在 `paths()` 块登记 2 个新 handler；在 `components(schemas)` 块登记 2 个新 DTO |
| `src/api/memorial_days/mod.rs` | **不动** | 不需要 re-export（handler 直接在 routes 模块下，openapi 用完整路径引用） |

**不修改**：
- `src/v3.sql`（不动 schema）
- `src/config.rs`
- 其他任何模块

---

## Task 1: 加 PinResponse / UnpinResponse DTO（含单测）

**Files:**
- Modify: `src/api/memorial_days/routes.rs:39-56`（在 `MemorialDayOut` 定义之后加 2 个新 DTO）
- Modify: `src/api/memorial_days/routes.rs` 文件末尾（加 `#[cfg(test)] mod tests` 块）
- Modify: `src/openapi.rs:255-260`（在 Memorial Days schemas 块加 2 个 DTO）

- [ ] **Step 1: 写失败的单测（DTO 序列化）**

在 `src/api/memorial_days/routes.rs` **最末尾**添加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_response_serializes_to_camel_case() {
        let resp = PinResponse {
            pinned_id: Some(100),
            pinned_at: Some("2026-06-19T10:30:00Z".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(
            json,
            r#"{"pinnedId":100,"pinnedAt":"2026-06-19T10:30:00Z"}"#
        );
    }

    #[test]
    fn unpin_response_serializes_to_null_pinned_id() {
        let resp = UnpinResponse { pinned_id: None };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"pinnedId":null}"#);
    }

    #[test]
    fn pin_response_optional_pinned_at() {
        let resp = PinResponse { pinned_id: None, pinned_at: None };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"pinnedId":null,"pinnedAt":null}"#);
    }
}
```

- [ ] **Step 2: 运行测试，验证失败**

Run: `cd D:\A-project\may_store && cargo test --lib memorial_days::routes::tests 2>&1 | head -30`
Expected: 编译错误 `cannot find type PinResponse`（因为 DTO 还没定义）

- [ ] **Step 3: 定义 PinResponse / UnpinResponse DTO**

在 `src/api/memorial_days/routes.rs` 第 56 行（`MemorialDayOut` 定义结束后）插入：

```rust
/// 置顶响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinResponse {
    /// 当前置顶的纪念日 ID（None = 没置顶）
    pub pinned_id: Option<i64>,
    /// 置顶时间（ISO8601 字符串，None = 没置顶）
    pub pinned_at: Option<String>,
}

/// 取消置顶响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnpinResponse {
    /// 置顶后该字段为 None
    pub pinned_id: Option<i64>,
}
```

- [ ] **Step 4: 运行测试，验证通过**

Run: `cd D:\A-project\may_store && cargo test --lib memorial_days::routes::tests 2>&1 | tail -20`
Expected: 3 个测试全 PASS

- [ ] **Step 5: 在 openapi.rs 注册新 DTO**

打开 `src/openapi.rs`，找到第 255-260 行的 Memorial Days schemas 块：

```rust
        // -- Memorial Days (纪念日) --
        // 之前只暴露了 input 类型,MemorialDayOut 没注册,前端自动生成时返回类型变 void
        // 这里补上 schema 让前端的 MemorialDay 类型能正确生成
        crate::api::memorial_days::routes::CreateMemorialDayInput,
        crate::api::memorial_days::routes::UpdateMemorialDayInput,
        crate::api::memorial_days::routes::MemorialDayOut,
```

在 `MemorialDayOut,` 后面加 2 行：

```rust
        crate::api::memorial_days::routes::MemorialDayOut,
        crate::api::memorial_days::routes::PinResponse,
        crate::api::memorial_days::routes::UnpinResponse,
```

- [ ] **Step 6: 验证编译**

Run: `cd D:\A-project\may_store && cargo check 2>&1 | tail -10`
Expected: 无错误（可能有未使用导入警告，可忽略）

- [ ] **Step 7: 提交**

```bash
cd D:\A-project\may_store && git add src/api/memorial_days/routes.rs src/openapi.rs && git commit -m "feat(memorial): add PinResponse/UnpinResponse DTO + openapi registration"
```

---

## Task 2: 实现 POST /pin 置顶 handler

**Files:**
- Modify: `src/api/memorial_days/routes.rs:21-37`（`configure()` 块加新 resource）
- Modify: `src/api/memorial_days/routes.rs:514`（文件末尾，辅助函数前）加新 handler

- [ ] **Step 1: 在 `configure()` 注册新路由**

打开 `src/api/memorial_days/routes.rs`，找到第 36 行（`web::resource("/api/groups/{group_id}/memorial-days/{id}")` 块结束的地方），**在第 36 行的 `);` 之后**插入：

```rust
    cfg.service(
        web::resource("/api/groups/{group_id}/memorial-days/{id}/pin")
            .route(web::post().to(pin_memorial_day))
            .route(web::delete().to(unpin_memorial_day)),
    );
```

- [ ] **Step 2: 实现 `pin_memorial_day` handler**

在 `delete_memorial_day` 函数（第 424 行 `Ok(ApiResponse::success(serde_json::json!({ "deleted": true })))}`）**之后**插入：

```rust
/// 置顶纪念日
/// POST /api/groups/{group_id}/memorial-days/{id}/pin
///
/// 行为：
/// - 事务里先清掉同组之前的置顶,再设新置顶（保证"一组同时只能 1 条"）
/// - 不存在 / 跨组 → 404
/// - 非组成员 → 403
/// - 成功 → 200 + { pinnedId, pinnedAt }
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/memorial-days/{id}/pin",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("id" = i64, Path, description = "纪念日 ID")
    ),
    responses(
        (status = 200, description = "置顶成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 404, description = "纪念日不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn pin_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    verify_group_member(&state, token.user_id, group_id).await?;

    let mut tx = state.db_pool.begin().await?;

    // 1) 清掉同组之前的置顶
    sqlx::query("UPDATE memorial_day SET is_default = 0 WHERE group_id = $1 AND is_default = 1")
        .bind(group_id)
        .execute(&mut *tx)
        .await?;

    // 2) 设新置顶
    let updated = sqlx::query(
        "UPDATE memorial_day SET is_default = 1 WHERE id = $1 AND group_id = $2",
    )
    .bind(id)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        // 回滚 + 404
        tx.rollback().await?;
        return Err(CustomError::resource_not_found("纪念日不存在"));
    }

    // 3) 取回置顶时间（updated_at）作为 pinnedAt 返回
    let row: (chrono::DateTime<chrono::Utc>,) = sqlx::query_as(
        "SELECT updated_at FROM memorial_day WHERE id = $1 AND group_id = $2",
    )
    .bind(id)
    .bind(group_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(ApiResponse::success(PinResponse {
        pinned_id: Some(id),
        pinned_at: Some(row.0.to_rfc3339()),
    }))
}
```

- [ ] **Step 3: 在 openapi.rs 注册新 handler 路径**

打开 `src/openapi.rs`，第 81-87 行的 Memorial Days paths 块：

```rust
        // ==================== Memorial Days (纪念日 §24.9) ====================
        crate::api::memorial_days::routes::list_memorial_days,
        crate::api::memorial_days::routes::create_memorial_day,
        crate::api::memorial_days::routes::get_memorial_day,
        crate::api::memorial_days::routes::update_memorial_day,
        crate::api::memorial_days::routes::delete_memorial_day,
        crate::api::memorial_days::routes::upcoming_memorial_days,
```

在 `upcoming_memorial_days,` 后面加 1 行：

```rust
        crate::api::memorial_days::routes::upcoming_memorial_days,
        crate::api::memorial_days::routes::pin_memorial_day,
```

- [ ] **Step 4: 验证编译**

Run: `cd D:\A-project\may_store && cargo check 2>&1 | tail -20`
Expected: 编译成功，可能有 dead_code 警告（因为 `unpin_memorial_day` 还没实现，下个任务加）

- [ ] **Step 5: 提交**

```bash
cd D:\A-project\may_store && git add src/api/memorial_days/routes.rs src/openapi.rs && git commit -m "feat(memorial): add POST /pin endpoint for pinning memorial day"
```

---

## Task 3: 实现 DELETE /pin 取消置顶 handler

**Files:**
- Modify: `src/api/memorial_days/routes.rs`（在 `pin_memorial_day` 函数之后加 `unpin_memorial_day`）

- [ ] **Step 1: 实现 `unpin_memorial_day` handler**

在 `pin_memorial_day` 函数**之后**（紧跟着）插入：

```rust
/// 取消置顶纪念日
/// DELETE /api/groups/{group_id}/memorial-days/{id}/pin
///
/// 行为：
/// - 把指定纪念日的 is_default 设为 0
/// - 不存在 / 跨组 / 本来就未置顶 → 一律返回 200（幂等，不泄露旁路信息）
/// - 非组成员 → 403
#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/memorial-days/{id}/pin",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("id" = i64, Path, description = "纪念日 ID")
    ),
    responses(
        (status = 200, description = "取消置顶成功（幂等）"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员")
    ),
    security(("bearer_auth" = []))
)]
pub async fn unpin_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    verify_group_member(&state, token.user_id, group_id).await?;

    // 幂等：不管该条纪念日存不存在、是 0 还是 1，都直接 UPDATE
    let _ = sqlx::query(
        "UPDATE memorial_day SET is_default = 0 WHERE id = $1 AND group_id = $2 AND is_default = 1",
    )
    .bind(id)
    .bind(group_id)
    .execute(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(UnpinResponse { pinned_id: None }))
}
```

- [ ] **Step 2: 在 openapi.rs 注册新 handler 路径**

打开 `src/openapi.rs`，找到刚才 Task 2 Step 3 加的那一行 `pin_memorial_day,`，**在它后面**加 1 行：

```rust
        crate::api::memorial_days::routes::pin_memorial_day,
        crate::api::memorial_days::routes::unpin_memorial_day,
```

- [ ] **Step 3: 验证编译**

Run: `cd D:\A-project\may_store && cargo check 2>&1 | tail -20`
Expected: 编译成功，0 警告（dead_code 警告应消失）

- [ ] **Step 4: 提交**

```bash
cd D:\A-project\may_store && git add src/api/memorial_days/routes.rs src/openapi.rs && git commit -m "feat(memorial): add DELETE /pin endpoint for unpinning memorial day"
```

---

## Task 4: 列表查询加 is_default DESC 排序

**Files:**
- Modify: `src/api/memorial_days/routes.rs:170-181`（`list_memorial_days` 里的 SELECT ORDER BY）
- Modify: `src/api/memorial_days/routes.rs:449-456`（`upcoming_memorial_days` 里的 SELECT ORDER BY）

- [ ] **Step 1: 改 `list_memorial_days` 的 ORDER BY**

打开 `src/api/memorial_days/routes.rs`，找到第 170-176 行的查询：

```rust
    let rows = sqlx::query(
        r#"SELECT id, group_id, name, description, memorial_date, calendar_type,
                  lunar_month, lunar_day, is_leap_month, is_default, created_at
           FROM memorial_day
           WHERE group_id = $1
           ORDER BY memorial_date ASC
           LIMIT $2"#,
    )
```

把 `ORDER BY memorial_date ASC` 改成 `ORDER BY is_default DESC, memorial_date ASC`：

```rust
           ORDER BY is_default DESC, memorial_date ASC
           LIMIT $2"#,
```

- [ ] **Step 2: 改 `upcoming_memorial_days` 的 ORDER BY**

找到第 449-455 行的查询：

```rust
    let rows = sqlx::query(
        r#"SELECT id, group_id, name, description, memorial_date, calendar_type,
                  lunar_month, lunar_day, is_leap_month, is_default, created_at
           FROM memorial_day
           WHERE group_id = $1
           ORDER BY memorial_date ASC"#,
    )
```

同样把 `ORDER BY memorial_date ASC` 改成 `ORDER BY is_default DESC, memorial_date ASC`：

```rust
           ORDER BY is_default DESC, memorial_date ASC"#,
```

- [ ] **Step 3: 验证编译**

Run: `cd D:\A-project\may_store && cargo check 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
cd D:\A-project\may_store && git add src/api/memorial_days/routes.rs && git commit -m "feat(memorial): order list/upcoming queries by is_default DESC then memorial_date"
```

---

## Task 5: 验证 PATCH 不暴露 is_default（不改代码，加注释 + 手动验）

**Files:**
- Modify: `src/api/memorial_days/routes.rs:70-80`（在 `UpdateMemorialDayInput` 定义上方的 doc 注释里加一句说明）

- [ ] **Step 1: 确认 PATCH 现有代码不暴露 is_default**

打开 `src/api/memorial_days/routes.rs`，检查 `UpdateMemorialDayInput` 结构体（第 70-80 行）：

```rust
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemorialDayInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub memorial_date: Option<NaiveDate>,
    pub calendar_type: Option<String>,
    pub lunar_month: Option<i16>,
    pub lunar_day: Option<i16>,
    pub is_leap_month: Option<bool>,
}
```

**确认该结构体里没有 `is_default` 字段**。serde 默认忽略未知字段,所以即使前端发 `{"isDefault": true}` 也会被静默丢弃。

- [ ] **Step 2: 确认 PATCH 的 SQL 也不更新 is_default**

检查第 355-361 行的动态 update 构建：

```rust
if input.name.is_some() { updates.push("name = $3"); }
if input.description.is_some() { updates.push("description = $4"); }
if input.memorial_date.is_some() { updates.push("memorial_date = $5"); }
if input.calendar_type.is_some() { updates.push("calendar_type = $6"); }
if input.lunar_month.is_some() { updates.push("lunar_month = $7"); }
if input.lunar_day.is_some() { updates.push("lunar_day = $8"); }
if input.is_leap_month.is_some() { updates.push("is_leap_month = $9"); }
```

**确认该列表里没有 `is_default` 字段**。

- [ ] **Step 3: 加注释说明设计意图**

在 `UpdateMemorialDayInput` 结构体**上方**的 doc 注释里加一句（如果没有 doc 注释则新建一个 `///` 块）：

```rust
/// 更新纪念日输入
///
/// **设计约束**:不接收 `isDefault` 字段。pin 走专门的 `POST /pin` 端点,
/// PATCH 只动 name/description/date/calendar 等普通字段,防止 PATCH 绕过 pin 流程。
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemorialDayInput {
```

- [ ] **Step 4: 验证编译**

Run: `cd D:\A-project\may_store && cargo check 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 5: 提交**

```bash
cd D:\A-project\may_store && git add src/api/memorial_days/routes.rs && git commit -m "docs(memorial): document that PATCH excludes is_default, must use /pin endpoint"
```

---

## Task 6: 端到端验证

**Files:** 无修改，纯验证

- [ ] **Step 1: cargo check 全量通过**

Run: `cd D:\A-project\may_store && cargo check 2>&1 | tail -5`
Expected: `Finished ... [unoptimized + debuginfo] target(s)` 且无 error

- [ ] **Step 2: cargo build --release 通过**

Run: `cd D:\A-project\may_store && cargo build --release 2>&1 | tail -5`
Expected: `Finished release [optimized] target(s)` 且无 error

- [ ] **Step 3: 单测全过**

Run: `cd D:\A-project\may_store && cargo test --lib memorial_days 2>&1 | tail -20`
Expected: 3 passed; 0 failed

- [ ] **Step 4: 手工 smoke test（需要本地能跑 dev server）**

按顺序执行并观察响应：

```bash
# 0) 启动 dev server
cd D:\A-project\may_store && cargo run

# 1) 登录拿 token (假设已有一个测试用户)
# 用 wx-login 拿到 access_token, 记为 $TOKEN
# 用 refresh/get_user_groups 拿到 group_id, 记为 $GID
# 创建 3 条纪念日,记录 id 分别为 $ID1, $ID2, $ID3

# 2) pin 第一条
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/api/groups/$GID/memorial-days/$ID1/pin"
# 期望: code=0, data.pinnedId=$ID1

# 3) 列列表,确认 $ID1 在最前 + isDefault=true
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/api/groups/$GID/memorial-days" | jq '.data[0]'
# 期望: id=$ID1, isDefault=true

# 4) pin 第二条 (验证切换)
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/api/groups/$GID/memorial-days/$ID2/pin"
# 期望: code=0, data.pinnedId=$ID2

# 5) 查 DB 确认 $ID1 的 is_default=0, $ID2 的 is_default=1
psql $DATABASE_URL -c "SELECT id, name, is_default FROM memorial_day WHERE group_id = $GID ORDER BY is_default DESC, memorial_date ASC"
# 期望: $ID2 排在第一 + is_default=1, $ID1 排在后面 + is_default=0

# 6) unpin
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/api/groups/$GID/memorial-days/$ID2/pin"
# 期望: code=0, data.pinnedId=null

# 7) 查 DB 确认所有 is_default=0
psql $DATABASE_URL -c "SELECT id, is_default FROM memorial_day WHERE group_id = $GID AND is_default = 1"
# 期望: 0 行

# 8) 试 PATCH 设 isDefault=true,确认被忽略
curl -X PATCH -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"hacked","isDefault":true}' \
  "http://localhost:8080/api/groups/$GID/memorial-days/$ID1"
# 期望: name 改成 "hacked",但 is_default 保持 0
```

- [ ] **Step 5: 打开 Swagger UI 确认新端点**

浏览器打开 `http://localhost:8080/docs`（或项目实际暴露的 Swagger 路径），找 "纪念日" tag 下：

- [ ] 能看到 `POST /api/groups/{group_id}/memorial-days/{id}/pin`
- [ ] 能看到 `DELETE /api/groups/{group_id}/memorial-days/{id}/pin`
- [ ] `MemorialDayOut` 响应里有 `isDefault` 字段

- [ ] **Step 6: 报告完成**

向用户汇报：
- 改了哪几个文件
- 加了哪几个端点
- 跑了哪些验证
- 截图 / 响应证据（如果做了手工测试）

---

## Self-Review Checklist（实施前最后过一遍）

- [x] Spec §4.1 (POST /pin) → Task 2 完整实现（含事务、错误码、utoipa path）
- [x] Spec §4.1 (DELETE /pin) → Task 3 完整实现（幂等返回 200、utoipa path）
- [x] Spec §4.2 (列表排序) → Task 4 改 2 个查询的 ORDER BY
- [x] Spec §4.2 (PATCH 不暴露 is_default) → Task 5 加注释 + 验证当前代码
- [x] Spec §4.3 (DTO) → Task 1 加 PinResponse/UnpinResponse + openapi 注册
- [x] Spec §6 (测试) → Task 1 写 3 个 DTO 单测；DB 行为在 Task 6 手工验证
- [x] Spec §7 (验收清单) → Task 6 端到端跑一遍
- [x] 没有 placeholder（TBD/TODO/伪代码）
- [x] 每个 step 有具体代码或命令
- [x] 文件路径精确
- [x] 类型 / 函数名在各 task 间一致（pin_memorial_day / unpin_memorial_day / PinResponse / UnpinResponse）
