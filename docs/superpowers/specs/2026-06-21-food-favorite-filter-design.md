# Foods 列表加「我的最爱」过滤

> **状态**：待实施（spec 待用户审）
> **作用域**：`GET /api/groups/{group_id}/foods` 加一个查询参数 `isFavorite`
> **不涉及**：数据库 schema 变更、其他端点

---

## 1. 背景（用户视角）

点单的人在组里翻"我点什么好"，不想翻一大堆他从来没点过 LIKE 的菜。
希望列表能多一个开关：**只看我自己点过 LIKE 的菜**。

---

## 2. 现状（已经有什么）

- 数据库已有 `user_food_mark` 表（`mark_type` 枚举：`LIKE` / `NOT_RECOMMEND`），注释直接叫"用户菜品标记/收藏"
- 列表项 `FoodSummary` 已经带 `isFavorited: bool`（当前用户是否给这道菜点过 LIKE）
- 详情 `FoodDetail` 同样带 `isFavorited`
- 标记接口 `POST /api/groups/{group_id}/foods/{food_id}/mark` + `DELETE` + `GET` 都已就位

**缺的就是列表端的"过滤"**。

---

## 3. 设计决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 参数名 | **`isFavorite`** | 用户已确认；语义直白 |
| 类型 | `Option<bool>` | 不传 = 不过滤；`true` = 只看我的最爱；`false` = 同不传 |
| 状态过滤 | `isFavorite=true` 时**强制 `status=ACTIVE`** | 隐藏 / 删除的菜不可能被下单，过滤出来没意义 |
| 与其他过滤关系 | 与 `tagId` / `keyword` / `cursor` / `limit` 是**与**关系 | 自由组合，互不干扰 |
| 排序 | **不动**（仍 `food_id DESC`） | 用户只要求"过滤"，没要求"置顶" |
| 返回结构 | **不动** `FoodListResponse` | 列表项里 `isFavorited` 恒为 `true`（被过滤出来的必然点过 LIKE） |
| 数据库 | **不动** | `user_food_mark` 表已经满足需求 |
| 接口契约 | 与现有 `status` 冲突时 | `isFavorite=true` 覆盖 `status`（最终按 ACTIVE 处理） |

---

## 4. SQL 改动（只在 `list_foods` 内）

当前 `list_foods` 的 WHERE 段：

```sql
WHERE f.group_id = $1
  AND ($2::food_status_enum IS NULL OR f.food_status = $2::food_status_enum)
  AND f.is_del = $3
  AND ($4::bigint IS NULL OR f.food_id < $4)
  AND ($5::bigint IS NULL OR f.tag_id = $5)
  AND ($6::text IS NULL OR f.food_name ILIKE $6 OR f.description ILIKE $6)
```

当 `isFavorite=true` 时**多一个 AND 条件**：

```sql
AND EXISTS (
  SELECT 1 FROM user_food_mark ufm
  WHERE ufm.user_id = $9 AND ufm.food_id = f.food_id AND ufm.mark_type = 'LIKE'
)
```

并且当 `isFavorite=true` 时，**强制把 `food_status_filter` 锁定为 `NORMAL`（即 API 层的 ACTIVE）**、`include_deleted = false`，忽略请求里的 `status` 参数。

> 实现细节：8 → 9 个 bind 占位符。`isFavorite=false` 或不传 → SQL 完全不变。

---

## 5. 用户视角能看到什么

### 5.1 调用方式（前端）

```
GET /api/groups/1/foods?isFavorite=true&limit=10
GET /api/groups/1/foods?tagId=5&isFavorite=true&limit=10
GET /api/groups/1/foods?keyword=牛&isFavorite=true&limit=10
```

不传 `isFavorite` 或 `isFavorite=false` → 行为完全等同现在。

### 5.2 返回

每条菜仍是同一个 `FoodSummary` 结构，`isFavorited: true`（因为被过滤出来的必然是我点过 LIKE 的）。

### 5.3 边界

- `isFavorite=true` 时菜品列表为空 → 正常返回 `foods: []`（不是错误）
- 翻页：仍用 cursor，第一次拿到的 `nextCursor` 继续翻
- 没点过任何 LIKE → 返回空列表（前端应该提示"还没有收藏的菜"）

---

## 6. OpenAPI / Swagger 改动

- `FoodListQuery` 结构加 `pub is_favorite: Option<bool>`，加文档注释
- `IntoParams` 自动同步（已带 macro）
- `/docs` 页自动刷新

---

## 7. 验收标准

- [ ] `GET .../foods?isFavorite=true` 只返回当前用户点过 LIKE 的菜
- [ ] `GET .../foods?isFavorite=true&tagId=5` 返回「tagId=5 且我点过 LIKE」的交集
- [ ] `GET .../foods?isFavorite=true&keyword=牛` 返回「名/描述含"牛"且我点过 LIKE」的交集
- [ ] `GET .../foods?isFavorite=true&status=HIDDEN` 仍只返回 ACTIVE（`isFavorite` 覆盖 `status`）
- [ ] 不传 `isFavorite` 或 `isFavorite=false` → 行为与现在完全一致
- [ ] Swagger UI 上能看到 `isFavorite` 参数
- [ ] v3.sql 不动

---

## 8. 不做什么（YAGNI）

- ❌ 不做"我收藏的菜"的独立入口接口（用户没要）
- ❌ 不做"按点赞数排序"（用户只说过滤）
- ❌ 不做"组内点赞总数"（`isFavorited` 已经是"我"的状态，不需要再加）
- ❌ 不重命名 `isFavorited` 字段（前端已对接）
