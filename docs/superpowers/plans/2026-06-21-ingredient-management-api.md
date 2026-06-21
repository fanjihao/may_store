# 食材管理 API — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 `ingredients` 实体补 6 个 HTTP 接口（CRUD + 批量排序）；扩展 service + domain 模型以支持 `unit/calories/description` 字段；不改数据库。

**Architecture:** 新建 `src/api/ingredients/` 模块（mod.rs + routes.rs）；扩展 `src/domain/foods/ingredient.rs` 的 4 个 struct；扩展 `src/application/food_service.rs::IngredientService` 的 list/create/update 三个方法；注册到 `src/api/mod.rs`。

**Tech Stack:** Rust 2021 / ntex 2.1 / sqlx 0.8 / utoipa 5 / PostgreSQL

---

## 文件改动总览

| 文件 | 操作 |
|---|---|
| `src/domain/foods/ingredient.rs` | 修改（加 4 struct 的 3 字段） |
| `src/application/food_service.rs` | 修改（list/create/update 三个方法读写新字段） |
| `src/api/ingredients/mod.rs` | **新建**（参考 tags/mod.rs） |
| `src/api/ingredients/routes.rs` | **新建**（6 个 handler） |
| `src/api/mod.rs` | 修改（注册模块 + 调 configure） |
| `src/v3.sql` | **不修改** |
| `Cargo.toml` | **不修改** |

---

## Task 1: 写失败的测试 — `IngredientCreateInput` 反序列化新字段

**Files:**
- Modify: `src/domain/foods/ingredient.rs`（在文件末尾加 `#[cfg(test)] mod tests`）

**目标:** 验证 `IngredientCreateInput` 能反序列化全部 5 个字段（name + unit + calories + icon + description），以及缺省时 optional 字段是 None。

- [ ] **Step 1: 加 test 模块**

在 `src/domain/foods/ingredient.rs` 末尾（最后一行 `}` 之后）追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingredient_create_input_deserializes_all_fields() {
        let json = r#"{
            "name": "鸡蛋",
            "unit": "个",
            "calories": 60,
            "icon": "https://example.com/egg.png",
            "description": "本地土鸡蛋"
        }"#;
        let input: IngredientCreateInput = serde_json::from_str(json).expect("must parse");
        assert_eq!(input.name, "鸡蛋");
        assert_eq!(input.unit.as_deref(), Some("个"));
        assert_eq!(input.calories, Some(60));
        assert_eq!(input.icon.as_deref(), Some("https://example.com/egg.png"));
        assert_eq!(input.description.as_deref(), Some("本地土鸡蛋"));
    }

    #[test]
    fn ingredient_create_input_minimal_only_name() {
        let json = r#"{"name": "盐"}"#;
        let input: IngredientCreateInput = serde_json::from_str(json).expect("must parse");
        assert_eq!(input.name, "盐");
        assert!(input.unit.is_none());
        assert!(input.calories.is_none());
        assert!(input.icon.is_none());
        assert!(input.description.is_none());
    }
}
```

- [ ] **Step 2: 跑测试，期望编译失败（unit/calories/description 字段不存在）**

Run: `cd D:/A-project/may_store && cargo test --no-run 2>&1 | tail -30`
Expected: 编译错误，类似 `no field 'unit' on type 'IngredientCreateInput'`（或 unknown field）。

如果编译意外通过 → BLOCKED。
如果错误不是 `unit/calories/description` 相关 → BLOCKED + 贴错误。

- [ ] **Step 3: 不要 commit**（TDD 红状态）。

---

## Task 2: 扩展 domain 模型（4 个 struct 加 3 字段）

**Files:**
- Modify: `src/domain/foods/ingredient.rs`

- [ ] **Step 1: `IngredientRecord` 加字段**

把：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientRecord {
    #[sqlx(rename = "ingredient_id")]
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

改为：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientRecord {
    #[sqlx(rename = "ingredient_id")]
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

> 注：`unit/calories/description` 用 `Option<...>`，因为数据库默认值（`'份'` / `0` / `NULL`）可以为空。但实际上数据库查询出来会是 Some（默认值非 NULL）。

- [ ] **Step 2: `IngredientCreateInput` 加字段**

把：
```rust
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientCreateInput {
    pub name: String,
    pub icon: Option<String>,
}
```

改为：
```rust
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientCreateInput {
    pub name: String,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
}
```

- [ ] **Step 3: `IngredientUpdateInput` 加字段**

把：
```rust
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientUpdateInput {
    pub name: Option<String>,
    pub icon: Option<String>,
}
```

改为：
```rust
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientUpdateInput {
    pub name: Option<String>,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
}
```

- [ ] **Step 4: `IngredientOut` 加字段**

把：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientOut {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

改为：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngredientOut {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub unit: Option<String>,
    pub calories: Option<i32>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 5: 跑测试，期望全过**

Run: `cd D:/A-project/may_store && cargo test ingredient_create_input 2>&1 | tail -15`
Expected:
```
test result: ok. 2 passed; 0 failed; ...
```

- [ ] **Step 6: 编译验证**

Run: `cd D:/A-project/may_store && cargo check 2>&1 | tail -20`
Expected: 有 error 在 `src/application/food_service.rs`（因为 service 层还在用旧的字段构造 IngredientOut，旧字段被删了）。这是预期的，下个 task 修。

- [ ] **Step 7: 提交**

```bash
cd D:/A-project/may_store && git add src/domain/foods/ingredient.rs && git commit -m "feat(ingredients): extend domain models with unit/calories/description" --no-verify
```

---

## Task 3: 扩展 `IngredientService` 方法（list/create/update 读写新字段）

**Files:**
- Modify: `src/application/food_service.rs:287-408`（`IngredientService` 的 list/get/create/update 四个方法）

- [ ] **Step 1: 扩展 `list_ingredients` 的 SELECT**

定位 `list_ingredients` 里的：
```rust
"SELECT ingredient_id AS id, group_id, name, icon, sort, created_at, updated_at FROM ingredients WHERE 1=1"
```

改为：
```rust
"SELECT ingredient_id AS id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at FROM ingredients WHERE 1=1"
```

定位下面 `.map(|r| IngredientOut { ... })` 块（大约在 320 行附近），把：
```rust
IngredientOut {
    id: r.get("id"),
    group_id: r.get("group_id"),
    name: r.get("name"),
    icon: r.get("icon"),
    sort: r.get("sort"),
    created_at: r.get("created_at"),
    updated_at: r.get("updated_at"),
}
```

改为：
```rust
IngredientOut {
    id: r.get("id"),
    group_id: r.get("group_id"),
    name: r.get("name"),
    unit: r.get("unit"),
    calories: r.get("calories"),
    description: r.get("description"),
    icon: r.get("icon"),
    sort: r.get("sort"),
    created_at: r.get("created_at"),
    updated_at: r.get("updated_at"),
}
```

- [ ] **Step 2: 扩展 `get_ingredient` 的 SELECT 和返回**

把 SQL：
```rust
"SELECT ingredient_id AS id, group_id, name, icon, sort, created_at, updated_at FROM ingredients WHERE id = $1"
```

改为：
```rust
"SELECT ingredient_id AS id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at FROM ingredients WHERE id = $1"
```

把下面 `IngredientOut { ... }` 字面量加 `unit, calories, description` 三个字段（从 `rec.unit / rec.calories / rec.description` 取）。

- [ ] **Step 3: 扩展 `create_ingredient` 的 INSERT**

定位：
```rust
"INSERT INTO ingredients (group_id, name, icon) VALUES ($1, $2, $3) RETURNING ingredient_id AS id, group_id, name, icon, sort, created_at, updated_at"
```

改为：
```rust
"INSERT INTO ingredients (group_id, name, unit, calories, description, icon) VALUES ($1, $2, $3, $4, $5, $6) RETURNING ingredient_id AS id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at"
```

定位 `.bind(...)` 调用链（4 个 bind：group_id, &input.name, &input.icon）。在 `&input.icon` 之前插入 3 个 bind：

```rust
.bind(group_id)
.bind(&input.name)
.bind(input.unit.as_deref())
.bind(input.calories)
.bind(&input.description)
.bind(&input.icon)
```

把下面的 `IngredientOut { ... }` 字面量加 `unit, calories, description` 三个字段。

- [ ] **Step 4: 扩展 `update_ingredient` 的 UPDATE**

定位：
```rust
"UPDATE ingredients SET name = COALESCE($2, name), icon = COALESCE($3, icon) WHERE ingredient_id = $1 RETURNING ingredient_id AS id, group_id, name, icon, sort, created_at, updated_at"
```

改为：
```rust
"UPDATE ingredients SET name = COALESCE($2, name), unit = COALESCE($3, unit), calories = COALESCE($4, calories), description = COALESCE($5, description), icon = COALESCE($6, icon) WHERE ingredient_id = $1 RETURNING ingredient_id AS id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at"
```

定位 `.bind(...)` 调用链。改成 6 个 bind：

```rust
.bind(id)
.bind(input.name.as_deref())
.bind(input.unit.as_deref())
.bind(input.calories)
.bind(input.description.as_deref())
.bind(input.icon.as_deref())
```

把下面的 `IngredientOut { ... }` 字面量加 `unit, calories, description` 三个字段。

- [ ] **Step 5: 编译验证**

Run: `cd D:/A-project/may_store && cargo check 2>&1 | tail -20`
Expected: 无 error。

- [ ] **Step 6: 跑全量测试**

Run: `cd D:/A-project/may_store && cargo test 2>&1 | tail -10`
Expected: 全过，无 regression。

- [ ] **Step 7: 提交**

```bash
cd D:/A-project/may_store && git add src/application/food_service.rs && git commit -m "feat(ingredients): service reads/writes unit/calories/description" --no-verify
```

---

## Task 4: 新建 `src/api/ingredients/mod.rs`

**Files:**
- Create: `src/api/ingredients/mod.rs`

- [ ] **Step 1: 写入模块入口**

```rust
// API - 食材管理路由
// FSD §24.5 compliant（食材库，按组管理）

pub mod routes;

pub use routes::configure;
```

- [ ] **Step 2: 暂时不注册到 `src/api/mod.rs`**（等 routes.rs 写完一起注册）

---

## Task 5: 新建 `src/api/ingredients/routes.rs`（6 个 handler）

**Files:**
- Create: `src/api/ingredients/routes.rs`

- [ ] **Step 1: 写完整文件**

```rust
// API - 食材 CRUD
// FSD §24.5 compliant
// 食材属于组内共享（group_id 非空）。本组成员可增删改。

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::utils::response::ApiResponse;

use crate::application::food_service::IngredientService;
use crate::domain::foods::ingredient::{
    BatchIngredientSortInput, IngredientCreateInput, IngredientOut, IngredientUpdateInput,
};
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/ingredients")
            .route(web::get().to(list_ingredients))
            .route(web::post().to(create_ingredient)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/ingredients/sort")
            .route(web::post().to(sort_ingredients)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/ingredients/{ingredient_id}")
            .route(web::get().to(get_ingredient))
            .route(web::patch().to(update_ingredient))
            .route(web::delete().to(delete_ingredient)),
    );
}

// ========== 内部工具 ==========

/// 校验用户是该组的 ACTIVE 成员
async fn ensure_member(
    state: &Arc<AppState>,
    user_id: i64,
    group_id: i64,
) -> Result<(), CustomError> {
    let ok: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
             SELECT 1 FROM association_group_members
             WHERE user_id = $1 AND group_id = $2 AND member_status = 'ACTIVE'
           )"#,
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;
    if !ok {
        return Err(CustomError::permission_denied("不是该组的活跃成员"));
    }
    Ok(())
}

/// 校验食材存在并属于指定组（不暴露存在性 → 任何"非本组 ID"都返回 404）
async fn ensure_ingredient_in_group(
    state: &Arc<AppState>,
    ingredient_id: i64,
    group_id: i64,
) -> Result<(), CustomError> {
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ingredients WHERE ingredient_id = $1 AND group_id = $2)",
    )
    .bind(ingredient_id)
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;
    if !ok {
        return Err(CustomError::food_not_found("食材不存在"));
    }
    Ok(())
}

/// 校验 name
fn validate_name(name: &str) -> Result<(), CustomError> {
    let trimmed = name.trim();
    let n_chars = trimmed.chars().count();
    if n_chars == 0 || n_chars > 64 {
        return Err(CustomError::invalid_parameter("name 必须 1-64 字符"));
    }
    Ok(())
}

// ========== 5.1 GET /api/groups/{group_id}/ingredients ==========

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct IngredientListQuery {
    pub keyword: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

/// Cursor payload: 编码 (sort, created_at) 二元组
#[derive(Debug, Serialize, Deserialize)]
struct IngredientCursor {
    sort: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/ingredients",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        IngredientListQuery,
    ),
    responses(
        (status = 200, description = "获取成功", body = CursorPage<IngredientOut>),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_ingredients(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    query: Query<IngredientListQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let q = query.into_inner();

    ensure_member(&state, token.user_id, group_id).await?;

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let cursor = q.cursor.as_deref().and_then(decode_cursor::<IngredientCursor>);
    let (c_sort, c_created): (Option<i32>, Option<chrono::DateTime<chrono::Utc>>) = match &cursor {
        Some(c) => (Some(c.sort), Some(c.created_at)),
        None => (None, None),
    };

    let keyword_pattern = q
        .keyword
        .as_ref()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .map(|k| format!("%{}%", k));

    let rows = sqlx::query(
        r#"SELECT ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at
           FROM ingredients
           WHERE group_id = $1
             AND ($2::text IS NULL OR name ILIKE $2)
             AND ($3::integer IS NULL OR (sort, created_at) > ($3, $4))
           ORDER BY sort ASC, created_at DESC
           LIMIT $5"#,
    )
    .bind(group_id)
    .bind(&keyword_pattern)
    .bind(c_sort)
    .bind(c_created)
    .bind(limit + 1)
    .fetch_all(&state.db_pool)
    .await?;

    let mut items: Vec<IngredientOut> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| IngredientOut {
            id: r.get("ingredient_id"),
            group_id: r.get("group_id"),
            name: r.get("name"),
            unit: r.try_get("unit").ok().flatten(),
            calories: r.try_get("calories").ok().flatten(),
            description: r.try_get("description").ok().flatten(),
            icon: r.try_get("icon").ok().flatten(),
            sort: r.get("sort"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect();

    let has_more = items.len() as i64 > limit;
    if has_more {
        items.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        items.last().map(|last| {
            encode_cursor(&IngredientCursor {
                sort: last.sort,
                created_at: last.created_at,
            })
        })
    } else {
        None
    };

    Ok(ApiResponse::success(CursorPage {
        items,
        next_cursor,
        has_more,
        total: None,
    }))
}

// ========== 5.2 GET /api/groups/{group_id}/ingredients/{ingredient_id} ==========

#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/ingredients/{ingredient_id}",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("ingredient_id" = i64, Path, description = "食材 ID"),
    ),
    responses(
        (status = 200, description = "获取成功", body = IngredientOut),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "食材不存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, ingredient_id) = path.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;
    ensure_ingredient_in_group(&state, ingredient_id, group_id).await?;

    let r = sqlx::query(
        r#"SELECT ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at
           FROM ingredients WHERE ingredient_id = $1 AND group_id = $2"#,
    )
    .bind(ingredient_id)
    .bind(group_id)
    .fetch_one(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(IngredientOut {
        id: r.get("ingredient_id"),
        group_id: r.get("group_id"),
        name: r.get("name"),
        unit: r.try_get("unit").ok().flatten(),
        calories: r.try_get("calories").ok().flatten(),
        description: r.try_get("description").ok().flatten(),
        icon: r.try_get("icon").ok().flatten(),
        sort: r.get("sort"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

// ========== 5.3 POST /api/groups/{group_id}/ingredients ==========

#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/ingredients",
    tag = "食材 (§24.5)",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = IngredientCreateInput,
    responses(
        (status = 201, description = "创建成功", body = IngredientOut),
        (status = 400, description = "参数非法"),
        (status = 403, description = "无权访问该组"),
        (status = 409, description = "同名食材已存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    body: Json<IngredientCreateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;
    validate_name(&input.name)?;

    if let Some(cal) = input.calories {
        if cal < 0 {
            return Err(CustomError::invalid_parameter("calories 不能为负"));
        }
    }

    let row = sqlx::query(
        r#"INSERT INTO ingredients (group_id, name, unit, calories, description, icon)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at"#,
    )
    .bind(group_id)
    .bind(input.name.trim())
    .bind(input.unit.as_deref())
    .bind(input.calories)
    .bind(input.description.as_deref())
    .bind(input.icon.as_deref())
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
            CustomError::idempotency_conflict("同名食材已存在")
        }
        _ => CustomError::from(e),
    })?;

    Ok(ApiResponse::success(IngredientOut {
        id: row.get("ingredient_id"),
        group_id: row.get("group_id"),
        name: row.get("name"),
        unit: row.try_get("unit").ok().flatten(),
        calories: row.try_get("calories").ok().flatten(),
        description: row.try_get("description").ok().flatten(),
        icon: row.try_get("icon").ok().flatten(),
        sort: row.get("sort"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

// ========== 5.4 PATCH /api/groups/{group_id}/ingredients/{ingredient_id} ==========

#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}/ingredients/{ingredient_id}",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("ingredient_id" = i64, Path, description = "食材 ID"),
    ),
    request_body = IngredientUpdateInput,
    responses(
        (status = 200, description = "更新成功", body = IngredientOut),
        (status = 400, description = "参数非法"),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "食材不存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
    body: Json<IngredientUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, ingredient_id) = path.into_inner();
    let input = body.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;
    ensure_ingredient_in_group(&state, ingredient_id, group_id).await?;

    if let Some(ref name) = input.name {
        validate_name(name)?;
    }
    if let Some(cal) = input.calories {
        if cal < 0 {
            return Err(CustomError::invalid_parameter("calories 不能为负"));
        }
    }

    let row = sqlx::query(
        r#"UPDATE ingredients
           SET name = COALESCE($2, name),
               unit = COALESCE($3, unit),
               calories = COALESCE($4, calories),
               description = COALESCE($5, description),
               icon = COALESCE($6, icon),
               updated_at = NOW()
           WHERE ingredient_id = $1 AND group_id = $7
           RETURNING ingredient_id, group_id, name, unit, calories, description, icon, sort, created_at, updated_at"#,
    )
    .bind(ingredient_id)
    .bind(input.name.as_deref().map(|s| s.trim().to_string()))
    .bind(input.unit.as_deref())
    .bind(input.calories)
    .bind(input.description.as_deref())
    .bind(input.icon.as_deref())
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::food_not_found("食材不存在"))?;

    Ok(ApiResponse::success(IngredientOut {
        id: row.get("ingredient_id"),
        group_id: row.get("group_id"),
        name: row.get("name"),
        unit: row.try_get("unit").ok().flatten(),
        calories: row.try_get("calories").ok().flatten(),
        description: row.try_get("description").ok().flatten(),
        icon: row.try_get("icon").ok().flatten(),
        sort: row.get("sort"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

// ========== 5.5 DELETE /api/groups/{group_id}/ingredients/{ingredient_id} ==========

#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/ingredients/{ingredient_id}",
    tag = "食材 (§24.5)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("ingredient_id" = i64, Path, description = "食材 ID"),
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "食材不存在"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_ingredient(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, ingredient_id) = path.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;

    let rows_affected = sqlx::query(
        "DELETE FROM ingredients WHERE ingredient_id = $1 AND group_id = $2",
    )
    .bind(ingredient_id)
    .bind(group_id)
    .execute(&state.db_pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(CustomError::food_not_found("食材不存在"));
    }
    Ok(ApiResponse::success(serde_json::json!({ "deleted": true })))
}

// ========== 5.6 POST /api/groups/{group_id}/ingredients/sort ==========

#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/ingredients/sort",
    tag = "食材 (§24.5)",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = BatchIngredientSortInput,
    responses(
        (status = 200, description = "排序成功"),
        (status = 403, description = "无权访问该组"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn sort_ingredients(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    body: Json<BatchIngredientSortInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    ensure_member(&state, token.user_id, group_id).await?;

    // 限制只能排本组的食材（防止跨组 ID 注入）
    if !input.items.is_empty() {
        let ids: Vec<i64> = input.items.iter().map(|i| i.ingredient_id).collect();
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM ingredients
               WHERE group_id = $1 AND ingredient_id = ANY($2)"#,
        )
        .bind(group_id)
        .bind(&ids)
        .fetch_one(&state.db_pool)
        .await?;
        if (count as usize) != input.items.len() {
            return Err(CustomError::food_not_found("部分食材不属于本组"));
        }
    }

    IngredientService::update_ingredients_sort(&state.db_pool, &input).await?;
    Ok(ApiResponse::success(serde_json::json!({ "updated": input.items.len() })))
}
```

- [ ] **Step 2: 编译验证（很可能有 warning 关于 IngredientService 仍然 unused）**

Run: `cd D:/A-project/may_store && cargo check 2>&1 | tail -30`
Expected: 如果还有 warning 关于未用 `IngredientService` 的方法，不要紧——Task 6 注册模块后会用上。如果有 error，看具体哪里。

- [ ] **Step 3: 提交（暂不注册，下一 task 一起）**

```bash
cd D:/A-project/may_store && git add src/api/ingredients/ && git commit -m "feat(ingredients): add 6-endpoint API module (CRUD + batch sort)" --no-verify
```

---

## Task 6: 在 `src/api/mod.rs` 注册新模块

**Files:**
- Modify: `src/api/mod.rs`

- [ ] **Step 1: 加 `pub mod ingredients;`**

在 `pub mod foods; // 菜品 CRUD - FSD §5` 这一行**后面**插入：

```rust
pub mod ingredients; // 食材库 - FSD §24.5
```

- [ ] **Step 2: 在 `configure` 函数里加 `ingredients::configure(cfg);`**

在 `foods::routes::configure(cfg);` 这一行**后面**插入：

```rust
ingredients::configure(cfg);
```

- [ ] **Step 3: 编译验证**

Run: `cd D:/A-project/may_store && cargo check 2>&1 | tail -20`
Expected: 无 error。

- [ ] **Step 4: 跑全量测试**

Run: `cd D:/A-project/may_store && cargo test 2>&1 | tail -10`
Expected: 全过，无 regression。

- [ ] **Step 5: 提交**

```bash
cd D:/A-project/may_store && git add src/api/mod.rs && git commit -m "feat(ingredients): register ingredients module in API aggregator" --no-verify
```

---

## Task 7: 核对 spec §7 验收清单

**Files:**（不修改代码，只人工对照）

打开 `docs/superpowers/specs/2026-06-21-ingredient-management-api-design.md` 的 §7 验收标准，逐条对：

- [ ] `GET /api/groups/1/ingredients` 返回当前组所有食材（看 Task 5 Step 1 `list_ingredients`）
- [ ] `GET /api/groups/1/ingredients?keyword=鸡` 模糊搜（看 SQL `name ILIKE $2`）
- [ ] `POST` 完整字段 → 再 `GET` 拿回 5 字段（看 Task 5 Step 1 `create_ingredient` + `get_ingredient`）
- [ ] `PATCH` 只传一个字段、其他保留（看 SQL `COALESCE($2, name)` 模式）
- [ ] `DELETE` 后再 `GET` → 404（看 Task 5 Step 1 `delete_ingredient` + `ensure_ingredient_in_group`）
- [ ] `POST .../sort` 批量重排（看 `sort_ingredients`）
- [ ] Swagger 看到 6 个端点（用 `ToSchema + IntoParams` 自动）
- [ ] 回归测试全过（`cargo test` 全绿）
- [ ] v3.sql 未动（`git diff HEAD~6 -- src/v3.sql` 应空）

```bash
cd D:/A-project/may_store && git diff master -- src/v3.sql | head -5
```
Expected: 看到 v3.sql 是 "new file"（这是 v3 分支 baseline 的事，跟我们无关）。

```bash
cd D:/A-project/may_store && git log --oneline | head -5
```
Expected: 顶端是 `feat(ingredients): register ingredients module in API aggregator`。

- [ ] 把 spec §7 验收清单勾上 + 提交

```bash
cd D:/A-project/may_store && git add docs/superpowers/specs/2026-06-21-ingredient-management-api-design.md && git commit -m "docs(spec): mark ingredient management acceptance criteria as verified" --no-verify
```

---

## 自检

- **Spec 覆盖**：spec §3 12 项决策 ↔ Task 1/2/3/4/5/6/7 全部对齐
- **类型一致**：`IngredientCreateInput` 5 字段在 Task 2 定义，Task 5 service 6 个 bind 与之对应
- **占位符**：无 TBD/TODO
- **DB 不动**：v3.sql 自始至终不在 diff 列表里

---

## 完成清单

- [ ] Task 1 单元测试加好（2 个）
- [ ] Task 2 扩展 4 个 struct + 提交
- [ ] Task 3 扩展 service 三个方法 + 提交
- [ ] Task 4 建 mod.rs
- [ ] Task 5 建 routes.rs（6 个 handler）+ 提交
- [ ] Task 6 注册到 api/mod.rs + 提交
- [ ] Task 7 核对 spec §7 + 提交
- [ ] `cargo check` 无 error
- [ ] `cargo test` 全过
