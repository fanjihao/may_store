# admin 用户管理 + 双人组管理完善 — 设计

> **状态**：待审（设计已通过用户口头批准，待 spec 文档过审）
> **作用域**：may_store 后端 + multi-admin 前端
> **不涉及**：数据库 schema 变更（复用 `users` / `association_groups` / `association_group_members` 现有字段）
> **取代**：两个 `<Alert type="info">` 占位提示

---

## 1. 背景与目标

**当前现状**：
- `multi-admin/src/features/store/users/UsersListPage.tsx` 和 `GroupsListPage.tsx` 都是占位提示符
- may_store 后端**只有只读列表接口**：`GET /api/admin/users`（LIMIT 100，无筛选/分页）、`GET /api/admin/groups`（同样 LIMIT 100）
- 后端**没有** PATCH user / PATCH group / GET group members 端点
- admin 想改一个用户的 nick_name 或 role、或想看一个组里有哪些成员 → **做不到**

**用户诉求**（人话版）：
- **用户管理**：能看 + 改 4 个字段：`nick_name`（昵称）、`role`（角色）、`status`（ACTIVE/BANNED/DELETED）、`username`（登录名）
- **双人组管理**：能看 + 改 `group_name`、能看组成员列表（只读）
- **每次改动写审计**：能查谁改了什么、什么时候改的、值是什么

---

## 2. 设计决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 改字段的 UX | Modal/Drawer 点行弹出 | 比行内编辑更适合多字段；跟现有 cat-i18n/languages 一致 |
| 用户改 4 个字段 | nick_name + role + status + username | 用户明确选定；包括 username（含唯一性校验） |
| 双人组改 1 个 + 看成员 | group_name（写）+ 成员列表（只读） | 用户明确选定 |
| 成员列表后端 | 加 `GET /api/admin/groups/{group_id}/members` | 干净独立；不与现有 list_users 耦合 |
| audit_logs 写入 | 每个 PATCH 都写（admin_users 表里 operator_id + action_type + detail 含前后值） | 与 `update_config` 现有模式一致 |
| audit_logs 写失败 | 不阻塞主更新（`let _ = ...`） | 与 `update_config` 现有模式一致 |
| 自我保护（admin 不能 disable 自己） | **不做** | "工具不该比用户更聪明"；admin_users 表与 users 表 status 字段独立 |
| 并发编辑 | last-write-wins，不加 version/etag | 100 用户/组规模撞不到；后续撞坑再补 |
| 权限粒度 | AdminToken 通过即可，不区分角色 | 与现有 admin 端点（audit-logs、compensate 等）一致 |
| 前端测试 | 不写 | 沿用上次"完善系统配置"的决定；项目无 vitest 基建 |
| 集成测试 | 不写 | 项目无 DB 基建 |

---

## 3. 数据模型

**无 schema 变更**。所有需要的字段已在 `users` / `association_groups` / `association_group_members` 中。

### `users` 表（v3.sql L327）

本次新增读写字段：

| 字段 | 业务语义 | 读 | 写 |
|---|---|---|---|
| `username` | 登录名（唯一） | ✓ | ✓（需唯一性校验） |
| `nick_name` | 昵称 | ✓ | ✓ |
| `role` | 角色（`user_role_enum`） | ✓ | ✓（白名单） |
| `status` | `ACTIVE` / `BANNED` / `DELETED`（`user_status_enum`） | ✓ | ✓（白名单） |

`role` 和 `status` 的合法 enum 值需在 `validate_user_update` 里硬编码白名单（不能依赖 sqlx 的 cast 在错误时返 400——它会 panic）。

### `association_groups` 表（v3.sql L385）

本次新增读写字段：

| 字段 | 业务语义 | 读 | 写 |
|---|---|---|---|
| `group_name` | 组名 | ✓ | ✓（非空、≤64） |

### `association_group_members` 表（v3.sql L426）

本次**只读**（用于 GET group members）：

| 字段 | 用途 |
|---|---|
| `user_id` | JOIN users 取 username/nick_name |
| `is_primary` | 是否主组标记 |
| `role_in_group` | 组内角色（`group_member_role_enum`） |
| `joined_at` | 加入时间 |
| `member_status` | 过滤 ACTIVE 成员 |

---

## 4. API

### 4.1 后端 may_store

#### 新增端点

##### `PATCH /api/admin/users/{user_id}`

- Auth: `AdminToken`（任何 admin）
- Body (`UserUpdateInput`，camelCase)：

```json
{
  "username": "alice",
  "nickName": "Alice",
  "role": "ORDERING",
  "status": "ACTIVE"
}
```

**所有字段都是可选**——只传要改的。**至少一个**非空字段，否则 400。

**响应 200**：更新后的 `UserOut`
```json
{
  "userId": 123,
  "username": "alice",
  "nickName": "Alice",
  "role": "ORDERING",
  "status": "ACTIVE",
  "lovePoint": 100,
  "diamond": 50,
  "createdAt": "2026-01-15T10:30:00Z"
}
```

**响应错误**：

| 状态 | 触发 |
|---|---|
| 400 | 字段非法（role/status 不在白名单、username 超长、nickName 空） |
| 401 | AdminToken 缺失或无效 |
| 403 | （预留，本次不用） |
| 404 | user_id 不存在 |
| 409 | username 已被其它用户占用 |

**事务逻辑**：
```sql
BEGIN;
  SELECT username FROM users WHERE user_id = $1 FOR UPDATE;  -- 锁行
  -- 校验 username 唯一性
  UPDATE users SET ... WHERE user_id = $1;
  -- 写 audit_logs (best-effort)
COMMIT;
```

##### `PATCH /api/admin/groups/{group_id}`

- Auth: `AdminToken`
- Body (`GroupUpdateInput`，camelCase)：

```json
{ "groupName": "My Group" }
```

**响应 200**：更新后的 `GroupOut`
```json
{
  "groupId": 100,
  "groupName": "My Group",
  "diamond": 200,
  "memberCount": 2,
  "createdAt": "2026-01-15T10:30:00Z"
}
```

**响应错误**：

| 状态 | 触发 |
|---|---|
| 400 | group_name 空 / 超 64 |
| 401 | token 缺失 |
| 404 | group_id 不存在 |

##### `GET /api/admin/groups/{group_id}/members`

- Auth: `AdminToken`
- 响应 200：`GroupMember[]`

```json
[
  {
    "userId": 1,
    "username": "alice",
    "nickName": "Alice",
    "isPrimary": true,
    "roleInGroup": "OWNER",
    "memberStatus": "ACTIVE",
    "joinedAt": "2026-01-15T10:30:00Z"
  },
  ...
]
```

仅返 `member_status='ACTIVE'` 的成员，按 `is_primary DESC, joined_at ASC`。

**响应错误**：

| 状态 | 触发 |
|---|---|
| 401 | token 缺失 |
| 404 | group_id 不存在（不影响 200 返空列表） |

#### 现有端点（不动）

- `GET /api/admin/users` — 现有只读列表
- `GET /api/admin/groups` — 现有只读列表

#### 新增模块

`src/api/admin/users.rs`（新）：
- `pub fn configure(cfg: &mut ServiceConfig)` —— 注册 PATCH 路由
- `pub fn validate_user_update(input: &UserUpdateInput) -> Result<(), CustomError>` —— 纯函数
- `pub async fn update_user(...)` —— handler
- `pub struct UserUpdateInput` —— 请求体
- `pub struct UserOut` —— 响应体（不与现有 `UserListItem` 合并，避免 scope 蔓延）
- `mod tests` —— 7 个单测

`src/api/admin/groups.rs`（新）：
- `pub fn configure(cfg: &mut ServiceConfig)` —— 注册 PATCH + GET members
- `pub fn validate_group_update(input: &GroupUpdateInput) -> Result<(), CustomError>` —— 纯函数
- `pub async fn update_group(...)` —— handler
- `pub async fn get_group_members(...)` —— handler
- `pub struct GroupUpdateInput`
- `pub struct GroupOut`
- `pub struct GroupMember`
- `mod tests` —— 4 个单测

`src/api/admin/mod.rs`（修改）：
- 加 `pub mod users;`
- 加 `pub mod groups;`

`src/api/admin/routes.rs`（修改）：
- 在 `configure(cfg: &mut ServiceConfig)` 顶部加 `users::configure(cfg);` 和 `groups::configure(cfg);`

### 4.2 前端 multi-admin

#### 修改

`src/features/store/users/UsersListPage.tsx`（占位 → 实现）：
- antd `Table` 显示 6 列：user_id、username、nick_name、role、status、love_point、diamond、created_at
- 每行"操作"列：编辑按钮
- 点击编辑 → `Modal` 打开（跟 cat-i18n/languages 同 pattern）
- Modal 内 `Form.useForm` 预填 4 字段，保存后 PATCH

`src/features/store/groups/GroupsListPage.tsx`（占位 → 实现）：
- antd `Table` 显示 5 列：group_id、group_name、diamond、member_count、created_at
- 每行"操作"列：编辑按钮
- 点击编辑 → `Modal` 打开
- Modal 内分两段：
  - **上**：编辑区（group_name + 保存）
  - **下**：成员列表（GET members 后渲染表格）

#### 新增

`src/features/store/users/hooks.ts`：
```ts
useUsers() — GET /api/admin/users
useUpdateUser() — PATCH /api/admin/users/{id}
```
（含 queryKey、staleTime、invalidate）

`src/features/store/users/types.ts`：
```ts
User（用 generated store.ts 的 schemas.UserOut）
UserUpdateInput（{ username?, nickName?, role?, status? }）
```

`src/features/store/groups/hooks.ts`：
```ts
useGroups() — GET /api/admin/groups
useUpdateGroup() — PATCH /api/admin/groups/{id}
useGroupMembers(groupId: number | null) — GET /api/admin/groups/{id}/members（groupId 为 null 时跳过请求）
```

`src/features/store/groups/types.ts`：
```ts
Group, GroupUpdateInput, GroupMember
```

#### 不动

- `router.tsx`（路由已存在）
- `ConfigPage`（已完成）
- 其它 store 页（dashboard、orders、wishes 等不在范围）

---

## 5. 文件改动清单

### may_store

| 文件 | 操作 |
|---|---|
| `src/api/admin/users.rs` | **新建**（handler + 校验纯函数 + types + tests） |
| `src/api/admin/groups.rs` | **新建**（2 个 handler + 校验纯函数 + types + tests） |
| `src/api/admin/mod.rs` | 修改（加 `pub mod users;` `pub mod groups;`） |
| `src/api/admin/routes.rs` | 修改（注册两个新子模块的路由） |
| `src/v3.sql` | **不修改** |
| `Cargo.toml` | **不修改**（无新依赖） |

### multi-admin

| 文件 | 操作 |
|---|---|
| `src/features/store/users/UsersListPage.tsx` | 修改（占位 → 表格 + Modal） |
| `src/features/store/groups/GroupsListPage.tsx` | 修改（占位 → 表格 + Modal + 成员区） |
| `src/features/store/users/hooks.ts` | **新建** |
| `src/features/store/users/types.ts` | **新建** |
| `src/features/store/groups/hooks.ts` | **新建** |
| `src/features/store/groups/types.ts` | **新建** |
| `src/router.tsx` | **不修改** |

---

## 6. 测试

### 6.1 后端单元测试（`cargo test --bin may-store`）

**`src/api/admin/users.rs` 末尾（新建 `mod tests`）**

| 用例 | 期望 |
|---|---|
| `validate_user_update_nickname_empty` | nick_name="" 拒 |
| `validate_user_update_role_invalid` | role="SuperAdmin" 拒 |
| `validate_user_update_role_valid` | role="ORDERING" 放行 |
| `validate_user_update_status_invalid` | status="banned" 拒 |
| `validate_user_update_status_valid` | status="ACTIVE" 放行 |
| `validate_user_update_username_empty` | username="" 拒 |
| `validate_user_update_username_too_long` | username>64 拒 |

**`src/api/admin/groups.rs` 末尾（新建 `mod tests`）**

| 用例 | 期望 |
|---|---|
| `validate_group_update_name_empty` | group_name="" 拒 |
| `validate_group_update_name_whitespace` | group_name="   " 拒（trim 后空） |
| `validate_group_update_name_too_long` | group_name>64 拒 |
| `validate_group_update_name_valid` | 正常放行 |

实现方式：跟 ConfigPage 一样，校验逻辑**抽出纯函数** `validate_user_update(...)` / `validate_group_update(...)`，单测直接调。

### 6.2 不测

- HTTP 集成测试（无 DB 基建）
- SQL 行为（手测）
- 前端组件（无 vitest 基建，按用户决定不写）

### 6.3 跑

```bash
cd /home/peter/project/may_store && cargo test --bin may-store
```

期望：原 58 个 + 新增 ~11 个 = **约 69 个**全过。

---

## 7. 验收清单

- [ ] `cargo check` / `cargo build --release` 通过
- [ ] `cargo test --bin may-store` 全过（含新增 ~11 个单测）
- [ ] PATCH `/api/admin/users/{id}` 接受 4 字段中任一组合、合法 → 200、不合法 → 400、username 冲突 → 409、不存在 → 404
- [ ] PATCH `/api/admin/groups/{id}` 接受 group_name、空/超长 → 400、不存在 → 404
- [ ] GET `/api/admin/groups/{id}/members` 返回 ACTIVE 成员列表、按 is_primary DESC, joined_at ASC
- [ ] 每次 PATCH 写 audit_logs，含 operator_id/operator_type='ADMIN'/action_type='USER_UPDATE' 或 'GROUP_UPDATE'/detail 含原值新值
- [ ] multi-admin /store/users 表格加载、点行编辑、保存、表格刷新
- [ ] multi-admin /store/groups 表格加载、点行编辑、看成员列表
- [ ] 失败时不关 Modal、字段下显示错误信息
- [ ] pnpm tsc / pnpm build 全过
- [ ] Swagger 中 UserOut / GroupOut / GroupMember schema 出现

---

## 8. 不在本次范围

- 用户/双人组的创建、删除（admin 现在只能改已有记录）
- 角色细分（SUPER_ADMIN / OPS / RISK_REVIEWER 不能改 user/ group，目前任何 admin 都能改）
- 自我保护（admin 不能 disable 自己的 users 记录）
- 并发编辑乐观锁（last-write-wins）
- 用户/双人组分页 / 搜索 / 筛选（前端 0 改动、后端不动现有 LIST 接口）
- 成员管理（admin 加/减组成员）—— 只读
- 软删除 / 封禁历史
- 前端单元测试
- 集成测试 / DB fixture