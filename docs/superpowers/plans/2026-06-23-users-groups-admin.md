# admin 用户管理 + 双人组管理 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 multi-admin 的"用户管理"和"双人组管理"两个占位页变成真的可读可改页面——后端加 PATCH/GET 端点、前端用表格 + Modal 编辑、每次改动写审计。

**Architecture:** 后端按子域拆 `src/api/admin/users.rs` 和 `src/api/admin/groups.rs`（参考已有 `audit_food` 等 handler 的子文件风格）；前端参考 ConfigPage 和 cat-i18n/languages 的 Modal + react-query 模式；校验逻辑抽纯函数 + 单测。

**Tech Stack:** Rust 2021 / ntex 2.1 / sqlx 0.8 / utoipa 5 / PostgreSQL 16 / React 19 / TypeScript 5 / antd 5 / @tanstack/react-query 5

---

## 文件改动总览

| 文件 | 操作 | 任务 |
|---|---|---|
| `src/api/admin/users.rs` | **新建** | T1, T2 |
| `src/api/admin/groups.rs` | **新建** | T3, T4, T5, T6 |
| `src/api/admin/mod.rs` | 修改（加 `pub mod users;` `pub mod groups;`） | T6 |
| `src/api/admin/routes.rs` | 修改（注册 3 个新路由） | T2, T6 |
| `src/v3.sql` | **不修改** | — |
| `Cargo.toml` | **不修改**（无新依赖） | — |
| `multi-admin/src/features/store/users/types.ts` | **新建** | T7 |
| `multi-admin/src/features/store/users/hooks.ts` | **新建** | T7 |
| `multi-admin/src/features/store/users/UsersListPage.tsx` | 修改（占位 → 实现） | T8 |
| `multi-admin/src/features/store/groups/types.ts` | **新建** | T9 |
| `multi-admin/src/features/store/groups/hooks.ts` | **新建** | T9 |
| `multi-admin/src/features/store/groups/GroupsListPage.tsx` | 修改（占位 → 实现） | T10 |
| `multi-admin/src/constants/api-paths.ts` | 修改（admin 路径补 `/api`） | T0 |

---

## Task 0: 前置 — 修 `api-paths.ts` 给 admin 端点加 `/api` 前缀

**Files:**
- Modify: `multi-admin/src/constants/api-paths.ts`

**目标:** `api-paths.ts` 里 `store.admin` 块的所有路径都**少了 `/api` 前缀**（之前 T8 修 `config` 时漏了其它）。may_store 实际端点都是 `/api/admin/...`。修这个，否则 T7/T9 用 `API_PATHS.store.admin.users` 会请求到错的 URL。

### Step 0.1: 读现状

```bash
grep -n "admin:" /home/peter/project/multi-admin/src/constants/api-paths.ts
```

确认 `store.admin` 块里所有路径。

### Step 0.2: 加 `/api` 前缀

把所有 `'/admin/...'` 改成 `'/api/admin/...'`。但**函数式**路径（带 `(id: number | string) =>`）保持箭头函数体、不动。示例：

```ts
admin: {
  stats: '/api/admin/stats',                     // 改
  users: '/api/admin/users',                     // 改
  groups: '/api/admin/groups',                   // 改
  groupConfigs: (id) => `/api/admin/groups/${id}/configs`, // 改
  groupDiamondsCompensate: (id) => `/api/admin/groups/${id}/diamonds/compensate`, // 改
  groupPointsCompensate: (id) => `/api/admin/groups/${id}/points/compensate`, // 改
  pendingOrders: '/api/admin/orders/pending-review', // 改
  orderReview: (id) => `/api/admin/orders/${id}/review`, // 改
  orderRewardReview: (id) => `/api/admin/orders/${id}/reward-review`, // 改
  wishQualityReward: (id) => `/api/admin/wishes/${id}/quality-reward`, // 改
  config: '/api/admin/configs',                  // 已经正确
  auditLogs: '/api/admin/audit-logs',            // 改
},
```

注意：只动 store.admin 块，**不要**改 cat.i18n 块的路径（cat 后端端口不一样，不在 /api 下）。

### Step 0.3: 类型检查

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit 2>&1 | tail -5
```

期望：干净

### Step 0.4: 提交

```bash
cd /home/peter/project/multi-admin && git add src/constants/api-paths.ts && git commit -m "fix(api-paths): add /api prefix to all store.admin paths (config endpoint was the only correct one)"
```

---

## Task 1: 后端 `users.rs` — 抽 `validate_user_update` 纯函数 + 7 个单测

**Files:**
- Create: `src/api/admin/users.rs`（先放 types + validate + tests，不含 handler）

**目标:** 把用户更新的白名单 + 范围校验抽成可独立测的纯函数 `validate_user_update`，handler（T2 再加）行为等价。跟 ConfigPage 的 `validate_config` 同模式。

### Step 1.1: 建文件 + types + validate + tests

新建 `src/api/admin/users.rs`，内容如下：

```rust
// API 层 - 后台用户管理

use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::utils::response::ApiResponse;

/// user_role_enum 合法值(查 v3.sql L332)
const VALID_ROLES: &[&str] = &["ORDERING", "RECEIVING", "ADMIN"];

/// user_status_enum 合法值(查 v3.sql L327)
const VALID_STATUSES: &[&str] = &["ACTIVE", "BANNED", "DELETED"];

/// 用户名最大长度(查 users.username VARCHAR(64))
const USERNAME_MAX_LEN: usize = 64;

/// 昵称最大长度(查 users.nick_name VARCHAR(64))
const NICK_NAME_MAX_LEN: usize = 64;

/// 用户更新输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserUpdateInput {
    pub username: Option<String>,
    pub nick_name: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
}

/// 用户输出(响应体)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserOut {
    pub user_id: i64,
    pub username: String,
    pub nick_name: Option<String>,
    pub role: String,
    pub status: String,
    pub love_point: i32,
    pub diamond: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 校验用户更新输入
pub fn validate_user_update(input: &UserUpdateInput) -> Result<(), CustomError> {
    // 至少一个字段
    if input.username.is_none()
        && input.nick_name.is_none()
        && input.role.is_none()
        && input.status.is_none()
    {
        return Err(CustomError::BadRequest(
            "至少需要更新一个字段".into(),
        ));
    }

    if let Some(ref u) = input.username {
        if u.is_empty() {
            return Err(CustomError::BadRequest("username 不能为空".into()));
        }
        if u.len() > USERNAME_MAX_LEN {
            return Err(CustomError::BadRequest(format!(
                "username 长度不能超过 {}",
                USERNAME_MAX_LEN
            )));
        }
    }

    if let Some(ref n) = input.nick_name {
        if n.is_empty() {
            return Err(CustomError::BadRequest("nickName 不能为空".into()));
        }
        if n.len() > NICK_NAME_MAX_LEN {
            return Err(CustomError::BadRequest(format!(
                "nickName 长度不能超过 {}",
                NICK_NAME_MAX_LEN
            )));
        }
    }

    if let Some(ref r) = input.role {
        if !VALID_ROLES.contains(&r.as_str()) {
            return Err(CustomError::BadRequest(format!(
                "role 必须是 {:?} 之一",
                VALID_ROLES
            )));
        }
    }

    if let Some(ref s) = input.status {
        if !VALID_STATUSES.contains(&s.as_str()) {
            return Err(CustomError::BadRequest(format!(
                "status 必须是 {:?} 之一",
                VALID_STATUSES
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_input() -> UserUpdateInput {
        UserUpdateInput {
            username: None,
            nick_name: None,
            role: None,
            status: None,
        }
    }

    #[test]
    fn validate_user_update_all_none_rejected() {
        let err = validate_user_update(&empty_input()).unwrap_err();
        assert!(format!("{}", err).contains("至少需要更新一个字段"));
    }

    #[test]
    fn validate_user_update_username_empty_rejected() {
        let input = UserUpdateInput {
            username: Some(String::new()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("username 不能为空"));
    }

    #[test]
    fn validate_user_update_username_too_long_rejected() {
        let input = UserUpdateInput {
            username: Some("a".repeat(USERNAME_MAX_LEN + 1)),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("username 长度"));
    }

    #[test]
    fn validate_user_update_nickname_empty_rejected() {
        let input = UserUpdateInput {
            nick_name: Some(String::new()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("nickName 不能为空"));
    }

    #[test]
    fn validate_user_update_role_invalid_rejected() {
        let input = UserUpdateInput {
            role: Some("SuperAdmin".to_string()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("role 必须是"));
    }

    #[test]
    fn validate_user_update_role_valid_passes() {
        let input = UserUpdateInput {
            role: Some("ORDERING".to_string()),
            ..empty_input()
        };
        assert!(validate_user_update(&input).is_ok());
    }

    #[test]
    fn validate_user_update_status_invalid_rejected() {
        let input = UserUpdateInput {
            status: Some("banned".to_string()),
            ..empty_input()
        };
        let err = validate_user_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("status 必须是"));
    }

    #[test]
    fn validate_user_update_status_valid_passes() {
        let input = UserUpdateInput {
            status: Some("ACTIVE".to_string()),
            ..empty_input()
        };
        assert!(validate_user_update(&input).is_ok());
    }
}
```

### Step 1.2: 跑测试（应该全过，因为 validate 函数 + tests 一起加了）

```bash
cd /home/peter/project/may_store && cargo test --bin may-store validate_user_update
```

期望：8 个测试全过（注意：上面"至少一个字段"算 1 个、加上其他 7 个 = 8 个测试，比 spec 写的 7 个多 1 个 —— 8 全过）

### Step 1.3: 检查 mod.rs

`src/api/admin/mod.rs` 当前是：
```rust
pub mod routes;
pub use routes::configure;
```

**先不**加 `pub mod users;`，等 T6 一起加（避免 T1 编译出错因为还没注册 route）。

### Step 1.4: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/users.rs && git commit -m "feat(admin-users): validate_user_update + 8 unit tests"
```

---

## Task 2: 后端 `users.rs` — `update_user` handler + 注册路由

**Files:**
- Modify: `src/api/admin/users.rs`（加 handler + configure fn）
- Modify: `src/api/admin/mod.rs`（加 `pub mod users;`）
- Modify: `src/api/admin/routes.rs`（调 `users::configure`）

### Step 2.1: 加 handler 到 `users.rs`

在 `src/api/admin/users.rs` 的 `validate_user_update` 函数**之后**、模块底部 tests **之前**，追加：

```rust
/// 配置路由(在 admin::routes::configure 里被调)
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/admin/users")
            .route("/{user_id}", web::patch().to(update_user)),
    );
}

/// PATCH /api/admin/users/{user_id}
///
/// 管理员修改用户字段。至少一个字段。改 username 需唯一性校验(409)。
/// 每次 PATCH 写 audit_logs(operator_id=admin, action_type='USER_UPDATE', detail 含原值/新值)
#[utoipa::path(
    patch,
    path = "/api/admin/users/{user_id}",
    tag = "后台管理 - 用户",
    params(("user_id" = i64, Path, description = "用户 ID")),
    request_body = UserUpdateInput,
    responses(
        (status = 200, description = "更新成功", body = UserOut),
        (status = 400, description = "字段非法"),
        (status = 401, description = "未登录"),
        (status = 404, description = "用户不存在"),
        (status = 409, description = "username 已被占用")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_user(
    state: State<Arc<AppState>>,
    admin: crate::middlewares::admin_auth::AdminToken,
    path: Path<i64>,
    body: Json<UserUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let user_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    validate_user_update(&input)?;

    let mut tx = db.begin().await?;

    // 锁行 + 读原值
    let row: Option<(String, Option<String>, String, String)> = sqlx::query_as(
        r#"SELECT username, nick_name, role::text, status::text
           FROM users WHERE user_id = $1 FOR UPDATE"#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (old_username, old_nick_name, old_role, old_status) = match row {
        Some(r) => r,
        None => return Err(CustomError::NotFound("用户不存在".into())),
    };

    // username 唯一性校验(如果改了)
    if let Some(ref new_username) = input.username {
        if new_username != &old_username {
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT user_id FROM users WHERE username = $1 AND user_id != $2 LIMIT 1",
            )
            .bind(new_username)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
            if exists.is_some() {
                return Err(CustomError::Conflict("该用户名已被使用".into()));
            }
        }
    }

    // 动态 UPDATE(只 SET 提供的字段)
    let new_username = input.username.clone().unwrap_or_else(|| old_username.clone());
    let new_nick_name = input.nick_name.clone().or_else(|| old_nick_name.clone());
    let new_role = input.role.clone().unwrap_or_else(|| old_role.clone());
    let new_status = input.status.clone().unwrap_or_else(|| old_status.clone());

    sqlx::query(
        r#"UPDATE users
           SET username = $1, nick_name = $2, role = $3::user_role_enum, status = $4::user_status_enum, updated_at = NOW()
           WHERE user_id = $5"#,
    )
    .bind(&new_username)
    .bind(&new_nick_name)
    .bind(&new_role)
    .bind(&new_status)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    // 写审计日志(best-effort, 跟 update_config 一致)
    let detail = serde_json::json!({
        "user_id": user_id,
        "before": {
            "username": old_username, "nick_name": old_nick_name,
            "role": old_role, "status": old_status,
        },
        "after": {
            "username": new_username, "nick_name": new_nick_name,
            "role": new_role, "status": new_status,
        },
    });
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, target_id, detail)
           VALUES ($1, 'ADMIN', 'USER_UPDATE', 'USER', $2, $3)"#,
    )
    .bind(admin.user_id)
    .bind(user_id)
    .bind(&detail)
    .execute(&mut *tx)
    .await;

    tx.commit().await?;

    // 重读返新值
    let out: (i64, String, Option<String>, String, String, i32, i32, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            r#"SELECT user_id, username, nick_name, role::text, status::text, love_point, diamond, created_at
               FROM users WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;

    Ok(ApiResponse::success(UserOut {
        user_id: out.0,
        username: out.1,
        nick_name: out.2,
        role: out.3,
        status: out.4,
        love_point: out.5,
        diamond: out.6,
        created_at: out.7,
    }))
}
```

### Step 2.2: 在 `src/api/admin/mod.rs` 加 `pub mod users;`

文件当前内容：
```rust
pub mod routes;
pub use routes::configure;
```

改为：
```rust
pub mod routes;
pub mod users;
pub use routes::configure;
```

### Step 2.3: 在 `src/api/admin/routes.rs` 的 `configure` 顶部加 `users::configure(cfg);`

定位 `routes.rs` 里 `pub fn configure(cfg: &mut ServiceConfig)` 函数，**第一行** `cfg.service(web::scope("/api/admin")...))` **之前**插入：

```rust
    users::configure(cfg);
```

注意：先 import。看看文件顶部的 use 段，把：
```rust
use crate::config::AppState;
```
后面或前面加：
```rust
use crate::api::admin::users;
```
(实际 `users` 在 `crate::api::admin::` 下, 同级 module 直接用 `super::users` 或 `users::`。先看 routes.rs 现有的 use 段，照着加。**注意** `super::users` 是子模块用法; 在 routes.rs 里因为已经在 `crate::api::admin::routes`, 所以 `super::users` 引用 `crate::api::admin::users`. **但更简单的是绝对路径** `crate::api::admin::users::configure(cfg);`)

实际：先 import `users` 模块,然后在 configure 顶部调。

具体写法参考 routes.rs 第 14 行附近的 use 段。看后照搬。

### Step 2.4: 跑 cargo check + test

```bash
cd /home/peter/project/may_store && cargo check 2>&1 | tail -10
cd /home/peter/project/may_store && cargo test --bin may-store 2>&1 | tail -5
```

期望：cargo check 干净（可能预存的 warnings）；cargo test 66 个全过（原 58 + T1 新增 8 = 66）

### Step 2.5: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/users.rs src/api/admin/mod.rs src/api/admin/routes.rs && git commit -m "feat(admin-users): PATCH /api/admin/users/{id} + audit log"
```

---

## Task 3: 后端 `groups.rs` — 抽 `validate_group_update` 纯函数 + 4 个单测

**Files:**
- Create: `src/api/admin/groups.rs`（先放 types + validate + tests）

### Step 3.1: 建文件 + types + validate + tests

新建 `src/api/admin/groups.rs`：

```rust
// API 层 - 后台双人组管理

use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::utils::response::ApiResponse;

/// 组名最大长度(查 association_groups.group_name VARCHAR(64))
const GROUP_NAME_MAX_LEN: usize = 64;

/// 组更新输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupUpdateInput {
    pub group_name: Option<String>,
}

/// 组输出(响应体)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupOut {
    pub group_id: i64,
    pub group_name: String,
    pub diamond: i64,
    pub member_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 组成员输出(GET members 响应体)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub user_id: i64,
    pub username: String,
    pub nick_name: Option<String>,
    pub is_primary: bool,
    pub role_in_group: String,
    pub member_status: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

/// 校验组更新输入
pub fn validate_group_update(input: &GroupUpdateInput) -> Result<(), CustomError> {
    if input.group_name.is_none() {
        return Err(CustomError::BadRequest(
            "至少需要更新一个字段(groupName)".into(),
        ));
    }
    let name = input.group_name.as_ref().unwrap().trim();
    if name.is_empty() {
        return Err(CustomError::BadRequest("groupName 不能为空".into()));
    }
    if name.len() > GROUP_NAME_MAX_LEN {
        return Err(CustomError::BadRequest(format!(
            "groupName 长度不能超过 {}",
            GROUP_NAME_MAX_LEN
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_group_update_none_rejected() {
        let input = GroupUpdateInput { group_name: None };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("至少需要更新一个字段"));
    }

    #[test]
    fn validate_group_update_empty_rejected() {
        let input = GroupUpdateInput {
            group_name: Some(String::new()),
        };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("groupName 不能为空"));
    }

    #[test]
    fn validate_group_update_whitespace_rejected() {
        let input = GroupUpdateInput {
            group_name: Some("   ".to_string()),
        };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("groupName 不能为空"));
    }

    #[test]
    fn validate_group_update_too_long_rejected() {
        let input = GroupUpdateInput {
            group_name: Some("a".repeat(GROUP_NAME_MAX_LEN + 1)),
        };
        let err = validate_group_update(&input).unwrap_err();
        assert!(format!("{}", err).contains("groupName 长度"));
    }

    #[test]
    fn validate_group_update_valid_passes() {
        let input = GroupUpdateInput {
            group_name: Some("My Group".to_string()),
        };
        assert!(validate_group_update(&input).is_ok());
    }
}
```

### Step 3.2: 跑测试

```bash
cd /home/peter/project/may_store && cargo test --bin may-store validate_group_update
```

期望：5 个测试全过

### Step 3.3: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/groups.rs && git commit -m "feat(admin-groups): validate_group_update + 5 unit tests"
```

---

## Task 4: 后端 `groups.rs` — `update_group` handler

**Files:**
- Modify: `src/api/admin/groups.rs`（加 handler）

### Step 4.1: 加 handler

在 `src/api/admin/groups.rs` 的 `validate_group_update` 函数**之后**、模块底部 tests **之前**，追加：

```rust
/// PATCH /api/admin/groups/{group_id}
///
/// 管理员修改双人组名。空名 / 超 64 → 400，不存在 → 404。
/// 每次 PATCH 写 audit_logs。
#[utoipa::path(
    patch,
    path = "/api/admin/groups/{group_id}",
    tag = "后台管理 - 双人组",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = GroupUpdateInput,
    responses(
        (status = 200, description = "更新成功", body = GroupOut),
        (status = 400, description = "groupName 非法"),
        (status = 401, description = "未登录"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_group(
    state: State<Arc<AppState>>,
    admin: crate::middlewares::admin_auth::AdminToken,
    path: Path<i64>,
    body: Json<GroupUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    validate_group_update(&input)?;
    let new_name = input.group_name.unwrap().trim().to_string();

    let mut tx = db.begin().await?;

    let old_name: Option<String> = sqlx::query_scalar(
        "SELECT group_name FROM association_groups WHERE group_id = $1 FOR UPDATE",
    )
    .bind(group_id)
    .fetch_optional(&mut *tx)
    .await?;

    let old_name = match old_name {
        Some(n) => n,
        None => return Err(CustomError::NotFound("组不存在".into())),
    };

    sqlx::query(
        "UPDATE association_groups SET group_name = $1, updated_at = NOW() WHERE group_id = $2",
    )
    .bind(&new_name)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;

    let detail = serde_json::json!({
        "group_id": group_id,
        "before": { "group_name": old_name },
        "after": { "group_name": new_name },
    });
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, target_id, detail)
           VALUES ($1, 'ADMIN', 'GROUP_UPDATE', 'ASSOCIATION_GROUP', $2, $3)"#,
    )
    .bind(admin.user_id)
    .bind(group_id)
    .bind(&detail)
    .execute(&mut *tx)
    .await;

    tx.commit().await?;

    let out: (String, i64, i32, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        r#"SELECT group_name, diamond, member_count, created_at
           FROM association_groups WHERE group_id = $1"#,
    )
    .bind(group_id)
    .fetch_one(db)
    .await?;

    Ok(ApiResponse::success(GroupOut {
        group_id,
        group_name: out.0,
        diamond: out.1,
        member_count: out.2,
        created_at: out.3,
    }))
}
```

### Step 4.2: 跑 check

```bash
cd /home/peter/project/may_store && cargo check 2>&1 | tail -5
```

期望：干净

### Step 4.3: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/groups.rs && git commit -m "feat(admin-groups): PATCH /api/admin/groups/{id} + audit log"
```

---

## Task 5: 后端 `groups.rs` — `get_group_members` handler

**Files:**
- Modify: `src/api/admin/groups.rs`（加 handler）

### Step 5.1: 加 handler

在 T4 加的 `update_group` 函数**之后**、模块底部 tests **之前**，追加：

```rust
/// GET /api/admin/groups/{group_id}/members
///
/// 返回 ACTIVE 成员列表，按 is_primary DESC, joined_at ASC。
/// 不存在 → 404。
#[utoipa::path(
    get,
    path = "/api/admin/groups/{group_id}/members",
    tag = "后台管理 - 双人组",
    params(("group_id" = i64, Path, description = "组 ID")),
    responses(
        (status = 200, description = "成员列表", body = Vec<GroupMember>),
        (status = 401, description = "未登录"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_group_members(
    state: State<Arc<AppState>>,
    _admin: crate::middlewares::admin_auth::AdminToken,
    path: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let db = &state.db_pool;

    // 检查组是否存在
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT group_id FROM association_groups WHERE group_id = $1",
    )
    .bind(group_id)
    .fetch_optional(db)
    .await?;

    if exists.is_none() {
        return Err(CustomError::NotFound("组不存在".into()));
    }

    let rows = sqlx::query(
        r#"SELECT m.user_id, u.username, u.nick_name, m.is_primary,
                  m.role_in_group::text, m.member_status::text, m.joined_at
           FROM association_group_members m
           JOIN users u ON u.user_id = m.user_id
           WHERE m.group_id = $1 AND m.member_status = 'ACTIVE'
           ORDER BY m.is_primary DESC, m.joined_at ASC"#,
    )
    .bind(group_id)
    .fetch_all(db)
    .await?;

    let members: Vec<GroupMember> = rows
        .iter()
        .map(|r| GroupMember {
            user_id: r.get("user_id"),
            username: r.get("username"),
            nick_name: r.get("nick_name"),
            is_primary: r.get::<i16, _>("is_primary") != 0,
            role_in_group: r.get("role_in_group"),
            member_status: r.get("member_status"),
            joined_at: r.get("joined_at"),
        })
        .collect();

    Ok(ApiResponse::success(members))
}
```

### Step 5.2: 跑 check + test

```bash
cd /home/peter/project/may_store && cargo check 2>&1 | tail -5
cd /home/peter/project/may_store && cargo test --bin may-store 2>&1 | tail -3
```

期望：cargo check 干净；cargo test 71 个全过（原 58 + 8 users + 5 groups = 71）

### Step 5.3: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/groups.rs && git commit -m "feat(admin-groups): GET /api/admin/groups/{id}/members"
```

---

## Task 6: 后端 — 注册 groups 路由 + mod.rs

**Files:**
- Modify: `src/api/admin/groups.rs`（加 `configure`）
- Modify: `src/api/admin/mod.rs`（加 `pub mod groups;`）
- Modify: `src/api/admin/routes.rs`（调 `groups::configure`）

### Step 6.1: 在 `groups.rs` 顶部加 configure

```rust
/// 配置路由(在 admin::routes::configure 里被调)
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/admin/groups")
            .route("/{group_id}", web::patch().to(update_group))
            .route("/{group_id}/members", web::get().to(get_group_members)),
    );
}
```

放在文件顶部 import 段之后、types 之前。

### Step 6.2: 在 `src/api/admin/mod.rs` 加 `pub mod groups;`

现在 mod.rs 是：
```rust
pub mod routes;
pub mod users;
pub use routes::configure;
```

改为：
```rust
pub mod groups;
pub mod routes;
pub mod users;
pub use routes::configure;
```

### Step 6.3: 在 `src/api/admin/routes.rs` 的 `configure` 顶部加 `groups::configure(cfg);`

在 T2.3 加的 `users::configure(cfg);` 那行**之前或之后**加：
```rust
    groups::configure(cfg);
```

（注意 import —— T2.3 已经加了 `users` 的 import, 这边要 import `groups` 同理。看 routes.rs 的 use 段，照搬。）

### Step 6.4: 跑 check + test

```bash
cd /home/peter/project/may_store && cargo check 2>&1 | tail -5
cd /home/peter/project/may_store && cargo test --bin may-store 2>&1 | tail -3
```

期望：cargo check 干净；71 个测试全过

### Step 6.5: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/groups.rs src/api/admin/mod.rs src/api/admin/routes.rs && git commit -m "feat(admin-groups): register PATCH + GET members routes"
```

---

## Task 7: 前端 users/types.ts + hooks.ts

**Files:**
- Create: `multi-admin/src/features/store/users/types.ts`
- Create: `multi-admin/src/features/store/users/hooks.ts`

### Step 7.1: 建 types.ts

新建 `multi-admin/src/features/store/users/types.ts`：

```ts
/**
 * 用户管理相关类型
 * 优先用 sync:types 出来的 OpenAPI schema（components['schemas']）
 * 这里只放本页面的辅助类型
 */
import type { components } from '@/api/generated/store';

/** GET /api/admin/users 响应项（来自 generated store.ts） */
export type UserListItem = components['schemas']['UserListItem'];

/** PATCH /api/admin/users/{id} 响应（来自 generated） */
export type UserOut = components['schemas']['UserOut'];

/** PATCH /api/admin/users/{id} 请求体 */
export interface UserUpdateInput {
  username?: string;
  nickName?: string;
  role?: string;
  status?: string;
}
```

注意：先 `cat /home/peter/project/multi-admin/src/api/generated/store.ts | grep -A 1 "UserOut\|UserListItem"` 确认 `components['schemas']` 里这两个 schema 真的存在。如果 generated store.ts 里只有 `UserListItem`（旧的 GET 响应）而没有 `UserOut`（新的 PATCH 响应），说明前端 sync types 落后于后端 —— **这种时候**先用 `unknown` 或跳过类型导入直接 inline type，最后 commit 一个 `chore: sync types` 单独处理。

实际步骤：先 grep 一下，**根据结果决定**：
- 如果都有 → 用上面的 import
- 如果只有 `UserListItem` 没有 `UserOut` → 临时用 `UserOut = UserListItem` 或直接 inline 一个 `interface UserOut { userId: number; username: string; ... }`

写文件时根据实际情况调整。**最稳的做法**：直接 inline `UserOut` interface（不依赖 sync types），避免 sync types 阻塞。

修正后的 types.ts（**不依赖 sync types**，安全）：

```ts
/**
 * 用户管理相关类型
 */
export interface UserListItem {
  userId: number;
  username: string;
  nickName?: string | null;
  role: string;
  lovePoint: number;
  diamond: number;
  createdAt: string;
}

export interface UserOut {
  userId: number;
  username: string;
  nickName?: string | null;
  role: string;
  status: string;
  lovePoint: number;
  diamond: number;
  createdAt: string;
}

export interface UserUpdateInput {
  username?: string;
  nickName?: string;
  role?: string;
  status?: string;
}
```

### Step 7.2: 建 hooks.ts

新建 `multi-admin/src/features/store/users/hooks.ts`：

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { storeHttp } from '@/adapters/store/http';
import { API_PATHS } from '@/constants/api-paths';
import type { UserListItem, UserOut, UserUpdateInput } from './types';

/** query key: 缓存用户列表 */
export const userKeys = {
  all: ['admin', 'users'] as const,
};

/** 拉用户列表 */
export function useUsers() {
  return useQuery({
    queryKey: userKeys.all,
    queryFn: async (): Promise<UserListItem[]> => {
      const resp = await storeHttp.get<unknown>(API_PATHS.store.admin.users);
      // 后端返 {code, message, data: UserListItem[]}, 经 storeHttp double-unwrap 后已是 UserListItem[]
      return (resp as { data?: UserListItem[] })?.data ?? (resp as UserListItem[]);
    },
    staleTime: 30_000,
  });
}

/** 改用户 */
export function useUpdateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (args: { userId: number; input: UserUpdateInput }) => {
      return storeHttp.patch<UserOut>(
        `${API_PATHS.store.admin.users}/${args.userId}`,
        args.input,
      );
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: userKeys.all });
    },
  });
}
```

注意：T8 里 `list_all_users` 之前**已经**有 GET 端点（admin/routes.rs L300 附近）。但**前端 sync 时不一定有** —— 看一下 `multi-admin/src/api/generated/store.ts` 里 `/api/admin/users` GET 的 schema。如果有 `UserListItem`，OK；如果没有，sync types（按之前 spec 流程跑 swagger 拉 openapi 然后 sync）。**但为了简化**，T7 假设**直接 inline 类型**（如 7.1 修正版），不依赖 sync。

API_PATHS 路径：检查 `multi-admin/src/constants/api-paths.ts`。如果 `users` 已经定义（之前 T8 检查过 store.admin.users），用。如果没，加 `users: '/api/admin/users'` 到 store.admin 块。

### Step 7.3: 类型检查

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit 2>&1 | tail -10
```

期望：干净

### Step 7.4: 提交

```bash
cd /home/peter/project/multi-admin && git add src/features/store/users/types.ts src/features/store/users/hooks.ts && git commit -m "feat(users-admin): add types.ts + hooks.ts (useUsers + useUpdateUser)"
```

---

## Task 8: 前端 `UsersListPage` 实现

**Files:**
- Modify: `multi-admin/src/features/store/users/UsersListPage.tsx`

### Step 8.1: 覆盖文件

```tsx
import { useState } from 'react';
import {
  App,
  Button,
  Form,
  Input,
  InputNumber,
  Modal,
  Select,
  Skeleton,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useUpdateUser, useUsers } from './hooks';
import type { UserListItem, UserOut, UserUpdateInput } from './types';

const { Title, Text } = Typography;

const ROLE_OPTIONS = [
  { value: 'ORDERING', label: 'ORDERING' },
  { value: 'RECEIVING', label: 'RECEIVING' },
  { value: 'ADMIN', label: 'ADMIN' },
];

const STATUS_OPTIONS = [
  { value: 'ACTIVE', label: 'ACTIVE' },
  { value: 'BANNED', label: 'BANNED' },
  { value: 'DELETED', label: 'DELETED' },
];

export function UsersListPage() {
  const { message } = App.useApp();
  const usersQuery = useUsers();
  const updateMutation = useUpdateUser();
  const [editing, setEditing] = useState<UserOut | null>(null);
  const [form] = Form.useForm<UserUpdateInput>();

  const openEdit = (user: UserListItem) => {
    // 编辑时用 UserOut 形状(包含 status 字段),但列表只返 UserListItem (无 status)
    // 后端 PATCH 响应会返完整 UserOut,这里先用列表项的字段预填,status 留空(后端会保留原值)
    setEditing({
      userId: user.userId,
      username: user.username,
      nickName: user.nickName,
      role: user.role,
      status: 'ACTIVE', // 默认;若想真值,需要后端在 UserListItem 也带 status
      lovePoint: user.lovePoint,
      diamond: user.diamond,
      createdAt: user.createdAt,
    });
    form.setFieldsValue({
      username: user.username,
      nickName: user.nickName ?? '',
      role: user.role,
      status: 'ACTIVE',
    });
  };

  const closeEdit = () => {
    setEditing(null);
    form.resetFields();
  };

  const onSubmit = async () => {
    if (!editing) return;
    const values = await form.validateFields();
    try {
      await updateMutation.mutateAsync({ userId: editing.userId, input: values });
      message.success('已保存');
      closeEdit();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      message.error(`保存失败:${msg}`);
    }
  };

  const columns: ColumnsType<UserListItem> = [
    { title: 'ID', dataIndex: 'userId', width: 80 },
    { title: '用户名', dataIndex: 'username', width: 140 },
    { title: '昵称', dataIndex: 'nickName', width: 140 },
    {
      title: '角色',
      dataIndex: 'role',
      width: 100,
      render: (v) => <Tag color={v === 'ADMIN' ? 'red' : 'blue'}>{v}</Tag>,
    },
    { title: '爱心积分', dataIndex: 'lovePoint', width: 100, align: 'right' },
    { title: '钻石', dataIndex: 'diamond', width: 80, align: 'right' },
    { title: '创建时间', dataIndex: 'createdAt', width: 180 },
    {
      title: '操作',
      width: 100,
      render: (_, row) => (
        <Button type="link" onClick={() => openEdit(row)}>
          编辑
        </Button>
      ),
    },
  ];

  if (usersQuery.isLoading) return <Skeleton active paragraph={{ rows: 6 }} />;
  if (usersQuery.isError) {
    return (
      <Card>
        <Text type="danger">加载失败:{String(usersQuery.error)}</Text>
      </Card>
    );
  }

  return (
    <div>
      <Space direction="vertical" size={4} style={{ marginBottom: 16 }}>
        <Title level={3} style={{ margin: 0 }}>用户管理</Title>
        <Text type="secondary">查看 / 修改用户昵称、角色、状态、用户名。</Text>
      </Space>

      <Table<UserListItem>
        rowKey="userId"
        columns={columns}
        dataSource={usersQuery.data ?? []}
        loading={usersQuery.isFetching}
        pagination={{ pageSize: 20 }}
      />

      <Modal
        title={`编辑用户 #${editing?.userId ?? ''}`}
        open={!!editing}
        onCancel={closeEdit}
        onOk={onSubmit}
        confirmLoading={updateMutation.isPending}
        destroyOnClose
      >
        <Form form={form} layout="vertical" preserve={false}>
          <Form.Item name="username" label="用户名" rules={[{ required: true, max: 64 }]}>
            <Input placeholder="登录名(唯一)" />
          </Form.Item>
          <Form.Item name="nickName" label="昵称" rules={[{ required: true, max: 64 }]}>
            <Input />
          </Form.Item>
          <Form.Item name="role" label="角色" rules={[{ required: true }]}>
            <Select options={ROLE_OPTIONS} />
          </Form.Item>
          <Form.Item name="status" label="状态" rules={[{ required: true }]}>
            <Select options={STATUS_OPTIONS} />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}

// Card 别名（仅本页用,避免 import 重复）
function Card({ children }: { children: React.ReactNode }) {
  return <div>{children}</div>;
}
```

注意：上面的 `Card` 是 stub,因为只为了不在 error 状态下引用 antd Card 而 import。**实际上不需要** Card wrapper,直接 `<div>` 即可 —— 把 `function Card(...) {...}` 那段**删掉**,直接:
```tsx
  if (usersQuery.isError) {
    return <Text type="danger">加载失败:{String(usersQuery.error)}</Text>;
  }
```

修正后的最终文件（删掉 Card stub）：

```tsx
import { useState } from 'react';
import {
  App,
  Button,
  Form,
  Input,
  Modal,
  Select,
  Skeleton,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useUpdateUser, useUsers } from './hooks';
import type { UserListItem, UserOut, UserUpdateInput } from './types';

const { Title, Text } = Typography;

const ROLE_OPTIONS = [
  { value: 'ORDERING', label: 'ORDERING' },
  { value: 'RECEIVING', label: 'RECEIVING' },
  { value: 'ADMIN', label: 'ADMIN' },
];

const STATUS_OPTIONS = [
  { value: 'ACTIVE', label: 'ACTIVE' },
  { value: 'BANNED', label: 'BANNED' },
  { value: 'DELETED', label: 'DELETED' },
];

export function UsersListPage() {
  const { message } = App.useApp();
  const usersQuery = useUsers();
  const updateMutation = useUpdateUser();
  const [editing, setEditing] = useState<UserOut | null>(null);
  const [form] = Form.useForm<UserUpdateInput>();

  const openEdit = (user: UserListItem) => {
    setEditing({
      userId: user.userId,
      username: user.username,
      nickName: user.nickName,
      role: user.role,
      status: 'ACTIVE',
      lovePoint: user.lovePoint,
      diamond: user.diamond,
      createdAt: user.createdAt,
    });
    form.setFieldsValue({
      username: user.username,
      nickName: user.nickName ?? '',
      role: user.role,
      status: 'ACTIVE',
    });
  };

  const closeEdit = () => {
    setEditing(null);
    form.resetFields();
  };

  const onSubmit = async () => {
    if (!editing) return;
    const values = await form.validateFields();
    try {
      await updateMutation.mutateAsync({ userId: editing.userId, input: values });
      message.success('已保存');
      closeEdit();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      message.error(`保存失败:${msg}`);
    }
  };

  const columns: ColumnsType<UserListItem> = [
    { title: 'ID', dataIndex: 'userId', width: 80 },
    { title: '用户名', dataIndex: 'username', width: 140 },
    { title: '昵称', dataIndex: 'nickName', width: 140 },
    {
      title: '角色',
      dataIndex: 'role',
      width: 100,
      render: (v) => <Tag color={v === 'ADMIN' ? 'red' : 'blue'}>{v}</Tag>,
    },
    { title: '爱心积分', dataIndex: 'lovePoint', width: 100, align: 'right' },
    { title: '钻石', dataIndex: 'diamond', width: 80, align: 'right' },
    { title: '创建时间', dataIndex: 'createdAt', width: 180 },
    {
      title: '操作',
      width: 100,
      render: (_, row) => (
        <Button type="link" onClick={() => openEdit(row)}>
          编辑
        </Button>
      ),
    },
  ];

  if (usersQuery.isLoading) return <Skeleton active paragraph={{ rows: 6 }} />;
  if (usersQuery.isError) {
    return <Text type="danger">加载失败:{String(usersQuery.error)}</Text>;
  }

  return (
    <div>
      <Space direction="vertical" size={4} style={{ marginBottom: 16 }}>
        <Title level={3} style={{ margin: 0 }}>用户管理</Title>
        <Text type="secondary">查看 / 修改用户昵称、角色、状态、用户名。</Text>
      </Space>

      <Table<UserListItem>
        rowKey="userId"
        columns={columns}
        dataSource={usersQuery.data ?? []}
        loading={usersQuery.isFetching}
        pagination={{ pageSize: 20 }}
      />

      <Modal
        title={`编辑用户 #${editing?.userId ?? ''}`}
        open={!!editing}
        onCancel={closeEdit}
        onOk={onSubmit}
        confirmLoading={updateMutation.isPending}
        destroyOnClose
      >
        <Form form={form} layout="vertical" preserve={false}>
          <Form.Item name="username" label="用户名" rules={[{ required: true, max: 64 }]}>
            <Input placeholder="登录名(唯一)" />
          </Form.Item>
          <Form.Item name="nickName" label="昵称" rules={[{ required: true, max: 64 }]}>
            <Input />
          </Form.Item>
          <Form.Item name="role" label="角色" rules={[{ required: true }]}>
            <Select options={ROLE_OPTIONS} />
          </Form.Item>
          <Form.Item name="status" label="状态" rules={[{ required: true }]}>
            <Select options={STATUS_OPTIONS} />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
```

### Step 8.2: 类型检查 + build

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit 2>&1 | tail -10
cd /home/peter/project/multi-admin && pnpm build 2>&1 | tail -5
```

期望：干净通过

### Step 8.3: 提交

```bash
cd /home/peter/project/multi-admin && git add src/features/store/users/UsersListPage.tsx && git commit -m "feat(users-admin): implement UsersListPage (table + edit modal)"
```

---

## Task 9: 前端 groups/types.ts + hooks.ts

**Files:**
- Create: `multi-admin/src/features/store/groups/types.ts`
- Create: `multi-admin/src/features/store/groups/hooks.ts`

### Step 9.1: 建 types.ts

新建 `multi-admin/src/features/store/groups/types.ts`：

```ts
/**
 * 双人组管理相关类型
 */
export interface GroupListItem {
  groupId: number;
  groupName: string;
  diamond: number;
  memberCount: number;
  createdAt: string;
}

export interface GroupOut {
  groupId: number;
  groupName: string;
  diamond: number;
  memberCount: number;
  createdAt: string;
}

export interface GroupUpdateInput {
  groupName?: string;
}

export interface GroupMember {
  userId: number;
  username: string;
  nickName?: string | null;
  isPrimary: boolean;
  roleInGroup: string;
  memberStatus: string;
  joinedAt: string;
}
```

### Step 9.2: 建 hooks.ts

新建 `multi-admin/src/features/store/groups/hooks.ts`：

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { storeHttp } from '@/adapters/store/http';
import { API_PATHS } from '@/constants/api-paths';
import type { GroupListItem, GroupMember, GroupOut, GroupUpdateInput } from './types';

export const groupKeys = {
  all: ['admin', 'groups'] as const,
  members: (groupId: number) => ['admin', 'groups', groupId, 'members'] as const,
};

/** 拉双人组列表 */
export function useGroups() {
  return useQuery({
    queryKey: groupKeys.all,
    queryFn: async (): Promise<GroupListItem[]> => {
      const resp = await storeHttp.get<unknown>(API_PATHS.store.admin.groups);
      return (resp as { data?: GroupListItem[] })?.data ?? (resp as GroupListItem[]);
    },
    staleTime: 30_000,
  });
}

/** 改双人组名 */
export function useUpdateGroup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (args: { groupId: number; input: GroupUpdateInput }) => {
      return storeHttp.patch<GroupOut>(
        `${API_PATHS.store.admin.groups}/${args.groupId}`,
        args.input,
      );
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: groupKeys.all });
    },
  });
}

/** 拉组成员(groupId 为 null 时跳过) */
export function useGroupMembers(groupId: number | null) {
  return useQuery({
    queryKey: groupId ? groupKeys.members(groupId) : ['admin', 'groups', 'noop'],
    queryFn: async (): Promise<GroupMember[]> => {
      if (!groupId) return [];
      const resp = await storeHttp.get<unknown>(
        `${API_PATHS.store.admin.groups}/${groupId}/members`,
      );
      return (resp as { data?: GroupMember[] })?.data ?? (resp as GroupMember[]);
    },
    enabled: groupId != null,
    staleTime: 30_000,
  });
}
```

注意 `API_PATHS.store.admin.groups` 路径 —— 之前 T8 检查过,这个 key 在 admin section 里有。如果报"找不到 key",先看 `constants/api-paths.ts` 是不是定义了,没的话加 `groups: '/api/admin/groups'`。

### Step 9.3: 类型检查

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit 2>&1 | tail -5
```

期望：干净

### Step 9.4: 提交

```bash
cd /home/peter/project/multi-admin && git add src/features/store/groups/types.ts src/features/store/groups/hooks.ts && git commit -m "feat(groups-admin): add types.ts + hooks.ts (useGroups/useUpdateGroup/useGroupMembers)"
```

---

## Task 10: 前端 `GroupsListPage` 实现

**Files:**
- Modify: `multi-admin/src/features/store/groups/GroupsListPage.tsx`

### Step 10.1: 覆盖文件

```tsx
import { useEffect, useState } from 'react';
import {
  App,
  Button,
  Form,
  Input,
  Modal,
  Skeleton,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useGroupMembers, useGroups, useUpdateGroup } from './hooks';
import type { GroupListItem, GroupMember, GroupOut, GroupUpdateInput } from './types';

const { Title, Text } = Typography;

export function GroupsListPage() {
  const { message } = App.useApp();
  const groupsQuery = useGroups();
  const updateMutation = useUpdateGroup();
  const [editingGroup, setEditingGroup] = useState<GroupOut | null>(null);
  const [form] = Form.useForm<GroupUpdateInput>();

  const groupIdForMembers = editingGroup?.groupId ?? null;
  const membersQuery = useGroupMembers(groupIdForMembers);

  const openEdit = (g: GroupListItem) => {
    setEditingGroup({
      groupId: g.groupId,
      groupName: g.groupName,
      diamond: g.diamond,
      memberCount: g.memberCount,
      createdAt: g.createdAt,
    });
    form.setFieldsValue({ groupName: g.groupName });
  };

  const closeEdit = () => {
    setEditingGroup(null);
    form.resetFields();
  };

  // Modal 打开时重置表单
  useEffect(() => {
    if (editingGroup) {
      form.setFieldsValue({ groupName: editingGroup.groupName });
    }
  }, [editingGroup, form]);

  const onSaveGroup = async () => {
    if (!editingGroup) return;
    const values = await form.validateFields();
    try {
      await updateMutation.mutateAsync({
        groupId: editingGroup.groupId,
        input: values,
      });
      message.success('已保存');
      // 不关 Modal,让用户继续看成员
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      message.error(`保存失败:${msg}`);
    }
  };

  const columns: ColumnsType<GroupListItem> = [
    { title: 'ID', dataIndex: 'groupId', width: 80 },
    { title: '组名', dataIndex: 'groupName' },
    { title: '成员数', dataIndex: 'memberCount', width: 100 },
    { title: '钻石', dataIndex: 'diamond', width: 100, align: 'right' },
    { title: '创建时间', dataIndex: 'createdAt', width: 180 },
    {
      title: '操作',
      width: 100,
      render: (_, row) => (
        <Button type="link" onClick={() => openEdit(row)}>
          编辑
        </Button>
      ),
    },
  ];

  const memberColumns: ColumnsType<GroupMember> = [
    { title: '用户 ID', dataIndex: 'userId', width: 80 },
    { title: '用户名', dataIndex: 'username', width: 140 },
    { title: '昵称', dataIndex: 'nickName', width: 140 },
    {
      title: '主组',
      dataIndex: 'isPrimary',
      width: 80,
      render: (v) => (v ? <Tag color="gold">主</Tag> : '-'),
    },
    { title: '组内角色', dataIndex: 'roleInGroup', width: 100 },
    { title: '加入时间', dataIndex: 'joinedAt', width: 180 },
  ];

  if (groupsQuery.isLoading) return <Skeleton active paragraph={{ rows: 6 }} />;
  if (groupsQuery.isError) {
    return <Text type="danger">加载失败:{String(groupsQuery.error)}</Text>;
  }

  return (
    <div>
      <Space direction="vertical" size={4} style={{ marginBottom: 16 }}>
        <Title level={3} style={{ margin: 0 }}>双人组管理</Title>
        <Text type="secondary">查看双人组、修改组名、查看成员。</Text>
      </Space>

      <Table<GroupListItem>
        rowKey="groupId"
        columns={columns}
        dataSource={groupsQuery.data ?? []}
        loading={groupsQuery.isFetching}
        pagination={{ pageSize: 20 }}
      />

      <Modal
        title={`编辑双人组 #${editingGroup?.groupId ?? ''}`}
        open={!!editingGroup}
        onCancel={closeEdit}
        footer={[
          <Button key="close" onClick={closeEdit}>关闭</Button>,
          <Button
            key="save"
            type="primary"
            loading={updateMutation.isPending}
            onClick={onSaveGroup}
          >
            保存组名
          </Button>,
        ]}
        width={720}
      >
        <Form form={form} layout="vertical">
          <Form.Item name="groupName" label="组名" rules={[{ required: true, max: 64 }]}>
            <Input placeholder="组名" />
          </Form.Item>
        </Form>

        <div style={{ marginTop: 24 }}>
          <Title level={5} style={{ marginTop: 0 }}>成员</Title>
          <Table<GroupMember>
            rowKey="userId"
            columns={memberColumns}
            dataSource={membersQuery.data ?? []}
            loading={membersQuery.isLoading}
            pagination={false}
            size="small"
          />
        </div>
      </Modal>
    </div>
  );
}
```

### Step 10.2: 类型检查 + build

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit 2>&1 | tail -5
cd /home/peter/project/multi-admin && pnpm build 2>&1 | tail -5
```

期望：干净通过

### Step 10.3: 提交

```bash
cd /home/peter/project/multi-admin && git add src/features/store/groups/GroupsListPage.tsx && git commit -m "feat(groups-admin): implement GroupsListPage (table + edit modal + members)"
```

---

## Task 11: 最终验证 + 最终 review

**Files:** — （不修改文件）

### Step 11.1: 后端

```bash
cd /home/peter/project/may_store && cargo check 2>&1 | tail -5
cd /home/peter/project/may_store && cargo build --release 2>&1 | tail -5
cd /home/peter/project/may_store && cargo test --bin may-store 2>&1 | tail -5
```

期望：3 个命令全过；cargo test 输出 `X passed; 0 failed`（期望约 71）

### Step 11.2: 前端

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit 2>&1 | tail -5
cd /home/peter/project/multi-admin && pnpm build 2>&1 | tail -10
```

期望：干净通过；build 成功

### Step 11.3: 验证 commits

may_store 应有 6 个新 commit（T1~T6）。

multi-admin 应有 4 个新 commit（T7~T10）。

### Step 11.4: 手动冒烟

1. 重启 may_store（让新代码生效）
2. 重启 multi-admin dev
3. 登录
4. /store/users：表格加载、点行编辑、改个昵称、保存、表格刷新
5. /store/groups：表格加载、点行编辑、修改 groupName、保存；下方成员列表显示 ACTIVE 成员
6. 改个越界值（如 username>64 字符）→ 后端 400 → 前端 toast 错误
7. DevTools Network → PATCH 请求 → Response Body 应是 ApiResponse envelope 含 data: UserOut

### Step 11.5: 最终 review

派 code reviewer 子 agent 看整次提交，找 spec 覆盖度、明显 bug、过度工程等问题。

---

## 完成标准

- [ ] T0: api-paths.ts 所有 admin 路径加 /api 前缀
- [ ] T1: 8 个 validate_user_update 单测全过
- [ ] T2: PATCH /api/admin/users/{id} 工作（含 audit_log）
- [ ] T3: 5 个 validate_group_update 单测全过
- [ ] T4: PATCH /api/admin/groups/{id} 工作（含 audit_log）
- [ ] T5: GET /api/admin/groups/{id}/members 工作
- [ ] T6: 所有路由注册、mod.rs 配齐
- [ ] T7: frontend users types + hooks
- [ ] T8: UsersListPage 表格 + Modal
- [ ] T9: frontend groups types + hooks
- [ ] T10: GroupsListPage 表格 + Modal + 成员区
- [ ] T11: cargo test / pnpm build 全过、手动冒烟通过

## 不在本次范围

- 用户/双人组的创建、删除（admin 现在只能改已有记录）
- 角色细分权限（任何 admin 都能改）
- 自我保护（admin 不能 disable 自己）
- 并发编辑乐观锁
- 用户/双人组分页 / 搜索 / 筛选
- 成员管理（admin 加/减组成员）—— 只读
- 前端单元测试
- 集成测试 / DB fixture