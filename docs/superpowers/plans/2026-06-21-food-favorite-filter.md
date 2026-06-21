# Foods 列表「我的最爱」过滤 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 `GET /api/groups/{group_id}/foods` 加 `isFavorite` 查询参数，传 `true` 时只返回当前用户点过 LIKE 的菜。

**Architecture:** 改 1 个文件（`src/api/foods/routes.rs`）。改 `FoodListQuery` struct 加字段、改 `list_foods` 函数体内构造 SQL 与状态过滤逻辑。不动数据库。

**Tech Stack:** Rust 2021 / ntex 2.1 / sqlx 0.8 (PostgreSQL) / utoipa 5

---

## 文件改动总览

| 文件 | 操作 | 责任 |
|---|---|---|
| `src/api/foods/routes.rs` | 修改 | 加查询参数 + 加 SQL EXISTS 子句 + 状态覆盖 |
| `docs/superpowers/specs/2026-06-21-food-favorite-filter-design.md` | 已写 | 设计依据（不再修改） |
| `src/v3.sql` | **不修改** | `user_food_mark` 表已满足 |

---

## Task 1: 写失败的单元测试 — `FoodListQuery` 反序列化 `isFavorite`

**Files:**
- Modify: `src/api/foods/routes.rs:780-820` (在文件末尾 `#[cfg(test)] mod tests` 块；如不存在则新建)

**目标:** 用一个纯 Rust 测试验证 URL 查询参数 `isFavorite=true` 正确反序列化为 `FoodListQuery.is_favorite = Some(true)`，以及未传时为 `None`、`=false` 时为 `Some(false)`。

- [ ] **Step 1: 在文件末尾加 test 模块**

确认文件末尾（行 815 之后）还没有 `#[cfg(test)] mod tests`，如果有就追加；没有就在末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_list_query_deserializes_is_favorite_true() {
        let q: FoodListQuery = serde_urlencoded::from_str("isFavorite=true&limit=10")
            .expect("must deserialize");
        assert_eq!(q.is_favorite, Some(true));
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn food_list_query_deserializes_is_favorite_false() {
        let q: FoodListQuery = serde_urlencoded::from_str("isFavorite=false")
            .expect("must deserialize");
        assert_eq!(q.is_favorite, Some(false));
    }

    #[test]
    fn food_list_query_omits_is_favorite_when_absent() {
        let q: FoodListQuery = serde_urlencoded::from_str("limit=20")
            .expect("must deserialize");
        assert_eq!(q.is_favorite, None);
        assert_eq!(q.limit, Some(20));
    }
}
```

- [ ] **Step 2: 跑测试，期望编译失败（field 不存在）**

Run: `cargo test --no-run 2>&1 | tail -30`
Expected: 编译错误，提示 `FoodListQuery` 没有 `is_favorite` 字段（`no field is_favorite on type FoodListQuery`）。

- [ ] **Step 3: 跑测试，期望 *运行* 失败**

如果编译过了则跳到 Task 2；如果确实编译失败 → OK，本步目的达到（测试在编译期红），继续 Task 2。

> 注：TDD 风格下，编译错误就等同于"测试失败"。等 Task 2 加完字段再跑一次会变绿。

---

## Task 2: 在 `FoodListQuery` 加 `is_favorite` 字段

**Files:**
- Modify: `src/api/foods/routes.rs:100-112`

- [ ] **Step 1: 修改 `FoodListQuery` struct**

把：

```rust
/// 列表查询参数 (FSD §5.2)
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct FoodListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<String>, // ACTIVE / HIDDEN / DELETED
    /// 按单个 tag_id 筛选
    pub tag_id: Option<i64>,
    /// 按菜品名/描述模糊搜索
    pub keyword: Option<String>,
}
```

改成：

```rust
/// 列表查询参数 (FSD §5.2)
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct FoodListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<String>, // ACTIVE / HIDDEN / DELETED
    /// 按单个 tag_id 筛选
    pub tag_id: Option<i64>,
    /// 按菜品名/描述模糊搜索
    pub keyword: Option<String>,
    /// 仅返回当前用户点过 LIKE 的菜（"我的最爱"过滤）；
    /// 传 true 时强制 status=ACTIVE，与 tag_id/keyword 是 AND 关系
    pub is_favorite: Option<bool>,
}
```

- [ ] **Step 2: 跑测试，期望 3 个测试都通过**

Run: `cargo test food_list_query 2>&1 | tail -30`
Expected:
```
test result: ok. 3 passed; 0 failed; ...
```

- [ ] **Step 3: 确认其他模块没编译过**

Run: `cargo check 2>&1 | tail -20`
Expected: 无 error。

- [ ] **Step 4: 提交**

```bash
git add src/api/foods/routes.rs
git commit -m "feat(foods): add is_favorite query param to FoodListQuery" --no-verify
```

---

## Task 3: 修改 `list_foods` — SQL 加 EXISTS 子句 + 状态覆盖

**Files:**
- Modify: `src/api/foods/routes.rs:390-450` (在 `list_foods` 函数体内)

- [ ] **Step 1: 替换状态过滤逻辑（在 status match 之前）**

定位到 `list_foods` 函数里：

```rust
// 状态过滤(默认仅 ACTIVE)
let want_status = q.status.as_deref().unwrap_or("ACTIVE");
let (food_status_filter, include_deleted) = match want_status {
    "DELETED" => (None, true),
    "HIDDEN" => (Some("OFF"), false),
    "ACTIVE" => (Some("NORMAL"), false),
    "AUDITING" => (Some("AUDITING"), false),
    "REJECTED" => (Some("REJECTED"), false),
    other => {
        return Err(CustomError::BadRequest(format!(
            "未知 status: {}",
            other
        )))
    }
};
```

替换为：

```rust
// 仅看"我的最爱"时强制 status=ACTIVE（覆盖请求里的 status 参数）；
// 不传或 false → 按用户传的 status 走原逻辑
let favorite_only = q.is_favorite.unwrap_or(false);
let (food_status_filter, include_deleted) = if favorite_only {
    (Some("NORMAL"), false)
} else {
    let want_status = q.status.as_deref().unwrap_or("ACTIVE");
    match want_status {
        "DELETED" => (None, true),
        "HIDDEN" => (Some("OFF"), false),
        "ACTIVE" => (Some("NORMAL"), false),
        "AUDITING" => (Some("AUDITING"), false),
        "REJECTED" => (Some("REJECTED"), false),
        other => {
            return Err(CustomError::BadRequest(format!(
                "未知 status: {}",
                other
            )))
        }
    }
};
```

- [ ] **Step 2: 改 SQL — 把整段 SELECT 提到 `let mut sql = String::from(...)`，按需追加 EXISTS 子句**

定位到 `list_foods` 里 `let rows = sqlx::query(...)` 起始的整段 SQL 字符串字面量（行 415–439）。把它替换为：

```rust
// 多取 1 行判 has_more
// 当 favorite_only 时多拼一个 AND EXISTS 子句，只看我点过 LIKE 的菜
let mut sql = String::from(
    r#"SELECT f.food_id, f.food_name, f.description, f.images, f.tag_id,
              t.tag_name, t.icon AS tag_icon,
              f.food_status::text AS food_status, f.is_del, f.created_by, f.group_id, f.created_at, f.updated_at,
              lo.last_order_at,
              lo.last_completed_at,
              EXISTS(SELECT 1 FROM user_food_mark ufm
                     WHERE ufm.user_id = $8 AND ufm.food_id = f.food_id AND ufm.mark_type = 'LIKE') AS is_favorited
       FROM foods f
       LEFT JOIN tags t ON t.tag_id = f.tag_id
       LEFT JOIN LATERAL (
         SELECT MAX(o.created_at) AS last_order_at,
                MAX(CASE WHEN o.status = 'CONFIRMED_COMPLETED' THEN o.updated_at END) AS last_completed_at
         FROM order_items oi
         JOIN orders o ON o.order_id = oi.order_id
         WHERE oi.food_id = f.food_id
       ) lo ON true
       WHERE f.group_id = $1
         AND ($2::food_status_enum IS NULL OR f.food_status = $2::food_status_enum)
         AND f.is_del = $3
         AND ($4::bigint IS NULL OR f.food_id < $4)
         AND ($5::bigint IS NULL OR f.tag_id = $5)
         AND ($6::text IS NULL OR f.food_name ILIKE $6 OR f.description ILIKE $6)"#,
);
if favorite_only {
    sql.push_str(
        r#"
         AND EXISTS (
           SELECT 1 FROM user_food_mark ufm_fav
           WHERE ufm_fav.user_id = $8
             AND ufm_fav.food_id = f.food_id
             AND ufm_fav.mark_type = 'LIKE'
         )"#,
    );
}
sql.push_str(
    r#"
       ORDER BY f.food_id DESC
       LIMIT $7"#,
);

let rows = sqlx::query(&sql)
    .bind(group_id)
    .bind(food_status_filter)
    .bind(if include_deleted { 1_i16 } else { 0_i16 })
    .bind(after_food_id)
    .bind(q.tag_id)
    .bind(&keyword_pattern)
    .bind(limit + 1)
    .bind(token.user_id)
    .fetch_all(&state.db_pool)
    .await?;
```

> 注意：把原先的 `sqlx::query(<literal>)` 改成 `sqlx::query(&sql)` —— sqlx 接受 `&str`，借用本地 String 即可。

- [ ] **Step 3: 编译验证**

Run: `cargo check 2>&1 | tail -30`
Expected: 无 error，无 warning（除非原有就有的）。

- [ ] **Step 4: 跑单元测试**

Run: `cargo test food_list_query 2>&1 | tail -20`
Expected:
```
test result: ok. 3 passed; 0 failed; ...
```

- [ ] **Step 5: 跑全量测试**

Run: `cargo test 2>&1 | tail -30`
Expected: 全部通过，没有 regression。

- [ ] **Step 6: 提交**

```bash
git add src/api/foods/routes.rs
git commit -m "feat(foods): filter list by current user's favorites when isFavorite=true

- Forces status=ACTIVE regardless of request's status param
- Adds AND EXISTS subquery against user_food_mark
- Falls back to original SQL when isFavorite is absent or false" --no-verify
```

---

## Task 4: 静态核对 spec 验收清单

**Files:**（不修改代码，只人工对照）

打开 `docs/superpowers/specs/2026-06-21-food-favorite-filter-design.md` 的 §7 验收标准，逐条对：

- [ ] `isFavorite=true` → 只返回当前用户点过 LIKE 的菜（看 SQL 子句 ✓）
- [ ] `isFavorite=true&tagId=5` → 交集（看 SQL 是 AND 关系 ✓）
- [ ] `isFavorite=true&keyword=牛` → 交集（看 SQL 是 AND 关系 ✓）
- [ ] `isFavorite=true&status=HIDDEN` → 仍只返回 ACTIVE（看 Task 3 Step 1 的 if 分支 ✓）
- [ ] 不传或 `=false` → 与原行为一致（看 favorite_only = false 走原 match ✓）
- [ ] Swagger 看到 `isFavorite`（看 Task 2 已用 `ToSchema + IntoParams`，会自动同步）
- [ ] v3.sql 未动（git diff 不应有 `src/v3.sql`）

Run:
```bash
git diff master -- src/v3.sql docs/superpowers/specs/2026-06-21-food-favorite-filter-design.md
```

Expected: 无输出（这俩文件没改）。

- [ ] **Step: 提交（如有微调）**

如果发现需要微调，回到 Task 3 调整后再跑测试。

---

## 自检（spec coverage / 类型一致性 / 占位符）

- **Spec 覆盖**：spec §3 的 8 个决策点 ↔ Task 1/2/3 全部对齐
- **类型一致**：`is_favorite: Option<bool>` 在 Task 2 定义，Task 3 用 `q.is_favorite.unwrap_or(false)` —— 类型一致
- **占位符**：无 TBD / TODO / "类似的"

---

## 完成清单

- [ ] Task 1 单元测试加好（3 个）
- [ ] Task 2 加 `is_favorite` 字段并提交
- [ ] Task 3 改 SQL + 状态逻辑并提交
- [ ] Task 4 核对 spec §7 全过
- [ ] `cargo test` 全绿
- [ ] `cargo check` 无新 warning
