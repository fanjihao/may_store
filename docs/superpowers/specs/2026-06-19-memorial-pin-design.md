# 纪念日置顶（is_default）设计

> **状态**：待审（设计已通过用户口头批准，待 spec 文档过审）
> **作用域**：纪念日模块，新增 2 个接口 + 调整 2 个现有查询的排序
> **不涉及**：数据库 schema 变更（复用现有 `is_default` 字段）
> **取代**：早前讨论的"系统默认 9 个里程碑 + 用户置顶"方案（已由用户简化为"只要用户置顶"）

---

## 1. 背景与目标

**当前现状**：
- `memorial_day` 表里有 `is_default SMALLINT` 字段（v3.sql 第 1267 行），但**没有任何代码路径把它设成 1**，等于只读 false
- `MemorialDayOut` 结构体已暴露 `is_default`（routes.rs:53），JSON 输出为 `isDefault`，但客户端**永远收到 false**

**用户诉求**（人话版）：
- 纪念日列表里会有很多（用户自己建的）
- 用户可以挑 1 条"置顶"
- 置顶的会显示在首页
- 一组同时只能有 1 个置顶
- 切换置顶 = 自动把旧的清掉
- 早先讨论的"系统自动生成 9 个里程碑"被砍掉

---

## 2. 设计决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 字段 | 复用现有 `is_default`（不改 schema） | 字段已存在、结构体已暴露、客户端可消费；新增字段纯属浪费 |
| 系统默认 9 个 | 砍掉 | 用户明确说"其他没啥要求，按你推荐的来" |
| API 形态 | 独立 POST/DELETE `/pin` 端点 | 切换置顶需事务原子性（先清后设），独立端点比 PATCH 字段更清晰 |
| 谁可以置顶 | 组内任意成员 | 与现有 CRUD 权限一致（`verify_group_member`） |
| 删除被置顶的 | 允许 | 记录没了置顶状态自然没了，最简单 |
| PATCH 被置顶的 | 允许，但不改 `is_default` | 字段走专门 pin 端点，PATCH 只动 name/date/calendar |
| 数量限制 | 1 条/组 | 用户选定 |

---

## 3. 数据模型

**`memorial_day` 表不变**。

`is_default` 字段当前定义：
```sql
is_default SMALLINT NOT NULL DEFAULT 0,
```

| 业务语义 | 值 |
|---|---|
| 普通纪念日 | 0 |
| 用户置顶 | 1（每组同时只能 1 条） |

**不需要新建索引**：现有 `idx_memorial_group(group_id)` 已覆盖"按组查所有"；每组只有 1 条 `is_default=1`，应用层"先清后设"即可保证唯一性。

---

## 4. API

### 4.1 新增端点

#### POST /api/groups/{group_id}/memorial-days/{id}/pin

- Tag: 纪念日 (§24.9)
- Auth: bearer_auth
- Path: `group_id` (i64), `id` (i64)
- Body: 无

**响应 200**：
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "pinnedId": 100,
    "pinnedAt": "2026-06-19T10:30:00Z"
  }
}
```

`pinnedAt` 取自 `is_default=1` 那条记录的 `updated_at`（与本事务的 UPDATE 时间一致）。

**响应错误**：

| 状态 | 触发 |
|---|---|
| 401 | token 失效（中间件） |
| 403 | 非组成员 |
| 404 | 纪念日不存在或不在该组 |
| 500 | DB 异常 |

**事务逻辑**：
```sql
BEGIN;
  UPDATE memorial_day SET is_default = 0
    WHERE group_id = $1 AND is_default = 1;   -- 1) 清旧置顶
  UPDATE memorial_day SET is_default = 1
    WHERE id = $2 AND group_id = $1;          -- 2) 设新置顶
  -- 影响 0 行 → 回滚 → 404
COMMIT;
```

#### DELETE /api/groups/{group_id}/memorial-days/{id}/pin

- Tag: 纪念日 (§24.9)
- Auth: bearer_auth
- Path: `group_id` (i64), `id` (i64)
- Body: 无

**响应 200**：
```json
{
  "code": 0,
  "message": "success",
  "data": { "pinnedId": null }
}
```

**逻辑**：
```sql
UPDATE memorial_day SET is_default = 0
  WHERE id = $1 AND group_id = $2 AND is_default = 1;
```

- 不存在的 id 也返回 200（no-op）
- 影响 0 行也 200（不泄露"是否置顶"的旁路信息）

### 4.2 现有端点改动

| 端点 | 改动 |
|---|---|
| `GET /api/groups/{group_id}/memorial-days` | SQL `ORDER BY` 改为 `is_default DESC, memorial_date ASC` |
| `GET /api/groups/{group_id}/memorial-days/upcoming` | SQL `ORDER BY` 改为同上 |
| `POST /api/groups/{group_id}/memorial-days` | 不动，INSERT 默认 `is_default = 0` |
| `PATCH /api/groups/{group_id}/memorial-days/{id}` | 不动现有字段；显式不接收 `isDefault`（防止 PATCH 绕过 pin 流程） |
| `DELETE /api/groups/{group_id}/memorial-days/{id}` | 不动。删除时 `is_default=1` 状态随记录一起消失 |

### 4.3 DTO

`MemorialDayOut` 已有 `pub is_default: bool`（routes.rs:53），JSON 输出为 `isDefault`，**不动**。

---

## 5. 文件改动清单

| 文件 | 改动 |
|---|---|
| `src/api/memorial_days/routes.rs` | 加 2 个新 handler + 1 个新路由配置 + 改 2 个查询的 ORDER BY |
| `src/openapi.rs` | 在 `#[openapi_paths]` 宏里登记新 handler（参考 `today_todos` 模式） |
| `src/api/memorial_days/mod.rs` | 可能要 re-export 新 handler（如果 openapi 收集需要） |

**不修改**：
- `src/v3.sql`
- `src/config.rs`
- 任何其他模块

---

## 6. 测试

`src/api/memorial_days/routes.rs` 文件底部 `#[cfg(test)] mod tests`：

| 用例 | 期望 |
|---|---|
| pin 第一条 | 200, 该条 `isDefault=true`，其它 `isDefault=false` |
| pin 第二条 | 200, 新条 `isDefault=true`，旧条 `isDefault=false` |
| pin 不存在的 id | 404 |
| pin 跨组 | 404（路径不匹配） |
| 非组成员 pin | 403 |
| DELETE 未置顶的 | 200, no-op |
| DELETE 置顶的 | 200, 该条 `isDefault=false` |
| 列表查询排序 | pin 在最前 + 其它按 memorial_date ASC |
| PATCH 不改 is_default | PATCH 改 name 后 is_default 保持原值 |
| 删除置顶的 | 200, 列表查询无该条 |

数据库测试需要 mock 或 fixture，本次**不**接入 sqlx::test（项目目前无此基础设施），改用单元测试 handler 输入校验 + 手工验证 DB 行为。

---

## 7. 验收清单

- [ ] v3.sql 未修改
- [ ] 新增 POST `/pin` 路由返回正确响应
- [ ] 新增 DELETE `/pin` 路由返回正确响应
- [ ] 列表查询 SQL 排序包含 `is_default DESC`
- [ ] upcoming 查询 SQL 排序包含 `is_default DESC`
- [ ] PATCH 端点不接收 `isDefault` 字段
- [ ] 切换置顶的事务原子性
- [ ] 非组员 403，跨组 404，不存在 404
- [ ] Swagger 中 `isDefault` 字段已暴露
- [ ] `cargo check` 通过
- [ ] `cargo build --release` 通过
- [ ] 单元测试覆盖核心场景

---

## 8. 不在本次范围

- 系统自动生成 9 个纪念日（"在一起 100 天"等）—— 已砍
- 多个用户置顶（>1 条）—— 当前每组限 1
- 置顶历史/审计（谁置顶过、什么时候取消）—— 当前仅记当前状态
- 首页卡片新接口（前端可基于现有 list + `isDefault` 字段自行实现）
- 多语言
