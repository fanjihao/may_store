# 食材管理 API 设计

> **状态**：待实施（spec 待用户审）
> **作用域**：补一组"食材库"CRUD + 批量排序的 HTTP 接口；扩展 service 层到包含 unit/calories/description
> **不涉及**：数据库 schema 变更（v3.sql 表已完整）、其他端点、其他实体

---

## 1. 背景（用户视角）

组的成员想管理"我家常用食材"——录入、查看、改、删、拖拽排序，让建菜时能快速选用。
现在后端数据库 `ingredients` 表 + service 层都写了，但**没暴露 HTTP 接口**，前端没法用。

---

## 2. 现状（已经有什么）

- **数据库表 `ingredients`**（v3.sql:569-583）：
  - 字段：`ingredient_id, name, group_id, unit, calories, description, icon, sort, created_at, updated_at`
  - 约束：`UNIQUE (name, group_id)`
  - 索引：`idx_ingredient_group ON ingredients(group_id)`
- **service 层 `IngredientService`**（`src/application/food_service.rs:287-434`）：
  - 已实现 `list_ingredients / get_ingredient / create_ingredient / update_ingredient / delete_ingredient / update_ingredients_sort`
  - 全部带 `#[allow(dead_code)]`，没人调
- **领域模型**（`src/domain/foods/ingredient.rs`）：
  - `IngredientRecord / IngredientCreateInput / IngredientUpdateInput / IngredientQuery / IngredientOut / BatchIngredientSortInput / IngredientSortItem`
  - **现有模型只覆盖 `name, icon` 两个字段**——service 层和模型都没接 `unit/calories/description`
- **HTTP 接口**：`src/api/` 下**没有 ingredients 模块**

---

## 3. 设计决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 路径风格 | 跟 tags 完全一致：`/api/groups/{group_id}/ingredients[/...]` | 项目内一致 |
| 隔离模型 | **每组独立**，group_id 强制非空 | 用户明确要求"每组独立" |
| 共享/全局 | **不做**全局食材（不像 tags 那样支持 NULL global） | 用户明确要求"每组独立" |
| 权限 | **本组任何成员都能增删改**，无需"创建者专属" | 用户明确要求；与 tags 一致 |
| 字段完整度 | 完整接 `name, unit, calories, description, icon` | 用户明确要求"加上 unit/calories/description" |
| 字段默认值 | `unit` 默认 `"份"`、`calories` 默认 `0`、`icon/description` 默认 None | 与 v3.sql 默认值对齐 |
| 排序 | **批量排序端点 `POST .../sort`** | 拖拽场景一次性传所有；service 层已有该方法 |
| 列表排序 | 按 `sort ASC, created_at DESC` | 跟 service 层现状一致 |
| 列表分页 | cursor（base64 编码 `sort+created_at` 二元组），默认 limit=50，最大 200 | 跟 tags 一致 |
| 错误返回 | 不存在 → 404 + `"食材不存在"`；非本组 → 404（不暴露存在性） | 跟 tags 一致 |
| 删除安全 | 直接 DELETE，**不**做引用检查 | `foods.ingredients` 是 JSON text，非 FK，不会级联；service 层现状如此 |
| 数据库 | **不动**（表已含全部字段） | v3.sql 不需要变更 |
| Swagger | 自动同步（用 `ToSchema + IntoParams`） | 跟 tags 一致 |

---

## 4. API 端点（6 个）

### 4.1 列表 `GET /api/groups/{group_id}/ingredients`

Query 参数：
- `keyword?: string` — 按名称模糊搜索
- `cursor?: string` — 上次响应的 nextCursor
- `limit?: i64` — 默认 50，最大 200

返回：`CursorPage<IngredientOut>`（项目内统一格式，含 items/nextCursor/hasMore/total）

### 4.2 详情 `GET /api/groups/{group_id}/ingredients/{ingredient_id}`

返回：`IngredientOut`
错误：`404` 不存在

### 4.3 创建 `POST /api/groups/{group_id}/ingredients`

Body (`IngredientCreateInput`)：
```json
{
  "name": "鸡蛋",          // 必填，1-64 字符
  "unit": "个",            // 可选，默认 "份"
  "calories": 60,          // 可选，默认 0
  "icon": "https://...",   // 可选
  "description": "本地土鸡蛋" // 可选
}
```

返回：`201` + `IngredientOut`
错误：`400` 校验失败、`409` 同名（UNIQUE 约束）

### 4.4 更新 `PATCH /api/groups/{group_id}/ingredients/{ingredient_id}`

Body (`IngredientUpdateInput`)：
```json
{
  "name": "土鸡蛋",        // 可选
  "unit": "个",            // 可选
  "calories": 70,          // 可选
  "icon": "https://...",   // 可选
  "description": "..."     // 可选
}
```

返回：`200` + `IngredientOut`
错误：`400` 校验失败、`404` 不存在

### 4.5 删除 `DELETE /api/groups/{group_id}/ingredients/{ingredient_id}`

返回：`200` + `{ "deleted": true }`
错误：`404` 不存在

### 4.6 批量排序 `POST /api/groups/{group_id}/ingredients/sort`

Body (`BatchIngredientSortInput`)：
```json
{
  "items": [
    { "ingredientId": 1, "sort": 0 },
    { "ingredientId": 2, "sort": 1 },
    { "ingredientId": 3, "sort": 2 }
  ]
}
```

返回：`200` + `{ "updated": <count> }`
行为：单事务，更新所有 sort

---

## 5. 文件改动总览

| 文件 | 操作 | 责任 |
|---|---|---|
| `src/domain/foods/ingredient.rs` | 修改 | 给 `IngredientRecord/IngredientCreateInput/IngredientUpdateInput/IngredientOut` 加 `unit/calories/description` 字段 |
| `src/application/food_service.rs` | 修改 | 扩展 `IngredientService` 的 `list/create/update` 方法读写 `unit/calories/description` |
| `src/api/ingredients/mod.rs` | **新建** | 模块入口（参考 `src/api/tags/mod.rs` 风格） |
| `src/api/ingredients/routes.rs` | **新建** | 6 个端点的 handler 实现 |
| `src/api/mod.rs` | 修改 | 注册 `ingredients` 模块 + 调 `configure` |
| `src/v3.sql` | **不修改** | 表结构已含全部字段 |
| `Cargo.toml` | **不修改** | 现有依赖足够（`serde_urlencoded` 已经在 dev-dep 里） |

---

## 6. 数据流 / 行为

### 6.1 用户视角（操作路径）

| 用户做什么 | 触发什么 |
|---|---|
| 打开"本组食材库"页面 | 前端调 `GET /api/groups/1/ingredients`，渲染列表（按 sort 顺序） |
| 点"+"新建"鸡蛋" | 前端调 `POST /api/groups/1/ingredients` body `{name, unit, calories, icon, description}`，后端返回新食材 |
| 点击某食材进详情 | 前端调 `GET /api/groups/1/ingredients/123` |
| 修改某字段 | 前端调 `PATCH /api/groups/1/ingredients/123` body 含要改的字段 |
| 拖拽重排 | 前端把新顺序一次性 POST 到 `/api/groups/1/ingredients/sort` |
| 删 | 前端调 `DELETE /api/groups/1/ingredients/123` |

### 6.2 边界 / 异常

- 用户不在组里 → 中间件 `RequireGroup` 拦（403/404）
- 删除不存在 → 404 `"食材不存在"`
- 创建同名（UNIQUE 约束违反） → 409 `"同名食材已存在"`
- `name` 为空或 > 64 字符 → 400
- `calories` 为负 → 400
- 排序 items 为空数组 → 200 noop

---

## 7. 验收标准

- [ ] `GET /api/groups/1/ingredients` 返回当前组所有食材，按 sort 排序
- [ ] `GET /api/groups/1/ingredients?keyword=鸡` 模糊搜出"鸡蛋/土鸡蛋"等
- [ ] `POST` 创建一个完整食材（含 unit/calories/description），再 GET 能拿回这 5 个字段
- [ ] `PATCH` 只传一个字段（如 unit），其他字段保留
- [ ] `DELETE` 后再 GET 详情 → 404
- [ ] `POST .../sort` 批量重排后，列表顺序确实变了
- [ ] Swagger `/docs` 上能看到全部 6 个端点和字段
- [ ] 现有回归测试全过
- [ ] v3.sql 没动
- [ ] 单元测试覆盖：query 反序列化、`unit` 默认值、`calories` 默认值、`name` 校验

---

## 8. 不做什么（YAGNI）

- ❌ 不做"全局食材"（跨组共享）
- ❌ 不做"按创建者限制权限"（用户要全员可改）
- ❌ 不做"软删除 / 回收站"（直接硬删，service 层现状）
- ❌ 不做"食材被菜品引用时禁止删除"（foods.ingredients 是 JSON text，没 FK）
- ❌ 不做"导入/导出"、"批量创建"（YAGNI）
- ❌ 不做"食材图片上传"（icon 用外链 URL，跟 tags 一致）
- ❌ 不重命名现有 service 方法 / 字段
