# 系统 Review 报告

**review 时间**: 2026-07-15
**review 范围**: may_store 后端 + wx-store 前端（端到端）
**review 方式**: 三路并行 review（后端 / 前端 / 集成）

---

## 总览

| 严重度 | 数量 | 含义 |
|--------|------|------|
| 🔴 P0 | 3 | 真 bug，用户能用脚投票 |
| 🟠 P1 | 8 | 契约/接缝问题，会随时间腐烂 |
| 🟡 P2 | 7 | 体验/文案/规范问题 |
| ✅ OK | 8 类 | 这次没发现问题的部分 |

---

## 🔴 P0：真 bug

### P0-1：`updateGuestRemark` 后端已注册但前端没 UI 调

- **位置**: 
  - 后端路由：`api/orders/routes.rs:41` 已挂载（`PATCH /api/orders/{order_id}/guest-remark`）
  - 前端 API 函数：`apis/order.ts:66` 已生成
  - 前端页面：**没有任何 .vue 调过**（grep `updateGuestRemark` 在 pages 下 0 命中）
- **现状**: 后端 OK，OpenAPI 也有这个接口，但前端没 UI 入口。**这不是路由没挂的 bug，是功能没做完**
- **影响**: "做客清单备注"功能（v3 FSD 7.10 提到）没法用
- **修复方向**:
  - 选 A：在 `createOrder.vue` / `orderDetail.vue` 加备注输入框 + 调 `updateGuestRemark`
  - 选 B：删掉前后端这一坨（如果短期不打算做）
- **修复成本**: 选 A 中等，选 B 1 行

### P0-2：`update_order_status` 事务 commit 后 `daily_cap_warning` 没序列化

- **位置**: `application/order_service.rs:1287` 构造 `OrderOutNew` 的地方
- **现状**: 之前对话里"完成订单超额提示用户"的修改，类型上是对的（`OrderOutNew` 有 `daily_cap_warning: Option<DailyCapWarning>` 字段），但**响应构造时没填这个字段**
- **影响**: 截断提示这个功能**根本没生效** —— 前端永远拿到 `daily_cap_warning: null`
- **修复成本**: 把 `daily_cap_warning` 变量传到 OrderOutNew 构造里

### P0-3：`unfreeze_wish_points` 仍只写流水不恢复 `users.love_point`

- **位置**: `application/wish_service.rs:839`
- **现状**: 拒绝心愿/心愿过期时调用此函数，但只 INSERT 一条 UNFREEZE 流水，**不实际更新 `users.love_point`**
- **影响**: 用户拒绝心愿时冻结的积分**没真退**，只是账上记一笔。"账面退、实际没退"的奇怪状态
- **修复成本**: 在 INSERT 流水后加一行 `UPDATE users SET love_point = love_point + frozen_amount`，并同步 `user_group_points`

---

## 🟠 P1：契约/接缝问题

### P1-1：12+ 个 WS 事件类型定义了但没 publish

- **位置**: `domain/event/types.rs:46-50`
- **现状**: `EventType::OrderReviewed / FootprintPublished / WishFulfilled / DiamondConsumed / PointChanged` 等 12+ 个事件类型定义了，但全仓 `EventPublisher::publish` 0 次调用
- **影响**: 死代码或功能未上线
- **修复成本**: 要么删（推荐），要么补 publish 点

### P1-2：订单状态变更没 WS 推送

- **位置**: `messages.rs` 定义了 `WsEnvelope::order_update` 但 0 publish
- **影响**: 订单/心愿页只能 `onShow` 轮询，对方一动自己这边看不到
- **修复成本**: 在 `order_service.rs::update_order_status` 事务 commit 后加 publish

### P1-3：`group_diamond_change` 只在签到时推

- **位置**: 订单完成路径没推
- **影响**: 前端 `composables/useGroup.ts:183` 订阅了但收不到订单场景的事件
- **修复成本**: 在 `order_service.rs` 推 `group_diamond_change`（如果订单确实会动钻石）

### P1-4：~~后端 60+ 个 API 前端没调用~~ 实际是 multi-admin 在用

- **位置**: `/home/peter/project/multi-admin` 是独立的后台管理项目
- **现状**（**之前的 review 错了**）: `multi-admin/src/api/generated/store.ts` 有 **106 个唯一 API 调用**，包括：
  - `/api/admin/audit-logs`、`/api/admin/configs`、`/api/admin/dashboard`
  - `/api/admin/foods/pending`、`/api/admin/foods/{food_id}/audit`
  - `/api/admin/group-levels`、`/api/admin/groups`
  - `/api/admin/orders/pending-review`、`/api/admin/orders/{order_id}/review`
  - 等等
- **影响**: 这部分 API **不是死代码**，是 multi-admin 在用。wx-store 用不到的 API（`/api/groups/{id}/settlement-check`、`/api/groups/{id}/fulfillment-stats`、`/api/wishes/pending-fulfillment` 等）需要逐个确认是 wx-store 后续要做、还是 multi-admin 漏了前端、还是真没人在用

### P1-5：前端写了 API 函数没挂页面

- **位置**: `apis/order.ts:13` 的 `getOrders` 等 4-5 处
- **影响**: 死代码
- **修复成本**: 删

### P1-6：状态机对不上：后端 SQL 用了 `BREEDER_FINISHED`

- **位置**: `api/users/today_todos.rs:312`
- **现状**: SQL `AND o.status IN ('PRODUCTION_COMPLETED'::order_status_enum,'BREEDER_FINISHED'::order_status_enum)` 用了 `BREEDER_FINISHED`
- **影响**: v3.sql `order_status_enum` 没有 `BREEDER_FINISHED`（只有 `PRODUCTION_COMPLETED`），PG enum cast 会炸
- **修复成本**: 1 行 SQL 改写

### P1-7：~~`swapRoleCheck` 前端没人调~~ 实际是有的，但 `disabled` 视觉锁有迷惑性

- **位置**: `pages/home/home.vue:486`（在 `handleSwitchRole` 内调 `swapRoleCheck`）
- **现状**（**之前的 review 错了**）:
  - 按钮 `:class="{ 'is-disabled': orderRes.ordersToHandle > 0 }"` 是**纯 CSS 视觉锁**（line 600 `.is-disabled` 样式），**不会阻止 `@tap` 触发**
  - 用户点了之后 `handleSwitchRole` 会跑，里面再调 `swapRoleCheck` 做完整预检（包括清单/协商中心愿/冻结积分等）
  - 如果预检不过，弹 modal 告诉用户具体原因
- **潜在问题**:
  1. 按钮被"锁"了用户也能点，UX 上有迷惑（看起来禁用了其实没禁用）
  2. `ordersToHandle > 0` 是基于本地 `orderRes`，不是后端权威数据
- **修复方向**:
  - 把 `disabled` 改成实际禁用（`:disabled="orderRes.ordersToHandle > 0"`）
  - 或者干脆去掉视觉锁，让 `swapRoleCheck` 弹 modal 兜底
- **修复成本**: 1 行

### P1-8：`reject_wish` / `close_wish_internal` 5 处重复未抽取

- **位置**: `application/wish_service.rs`
- **影响**: 维护成本高，下次改一处要改多处
- **修复成本**: 中等，抽取公共函数

---

## 🟡 P2：体验/文案/规范

### P2-1：`feedback.vue` 加载失败只 `console.error`

- **位置**: `pages/feedback/feedback.vue:275`
- **影响**: 用户看到黑屏还以为在加载
- **修复成本**: 1 行 toast

### P2-2：`todayslist.vue` 是 TODO 空壳

- **位置**: `pages/todayslist/todayslist.vue:191,209`
- **影响**: 永远停在"当天暂无菜品"
- **修复成本**: 要么实现要么删

### P2-3：`as any` 滥用

- **位置**: `pages/stash/stash.vue:411`、`pages/wishDetail/wishDetail.vue:217`
- **影响**: 失去类型保护，OpenAPI 自动生成的类型没真用上
- **修复成本**: 改类型即可

### P2-4：注释里还有"接单人/下单人/订单"

- **位置**:
  - `pages/wishDetail/wishDetail.vue:163,176,180`
  - `pages/pointExchange/pointExchange.vue:606,617,627`
- **影响**: 跟用户面"吃货/饲养员/清单"对不上，**只在代码注释里**不影响用户
- **修复成本**: 全局替换

### P2-5：`e?.errMsg` 原文吐给用户

- **位置**: `pages/feedback/feedback.vue:213`、`pages/order/order.vue:382` 等多处
- **影响**: 后端技术错误信息漏出来
- **修复成本**: 全局错误拦截器

### P2-6：`home.vue:57` 写死"累计投喂 0 次"

- **位置**: `pages/home/home.vue:57`
- **影响**: 是 placeholder，后续补字段时记得接入
- **修复成本**: 接入真实数据源

### P2-7：`wish_service.rs:657` `WishQualityRewarded` 推了但没接 WS handler

- **位置**: 后端事件 + 前端订阅
- **影响**: 后端推了前端没人收
- **修复成本**: 前端加订阅

---

## ✅ 没发现问题的部分

- **API 路由注册完整性**（除 P0-1 `guest-remark`）— 主要业务接口都注册了
- **DDL/enum 全在 v3.sql** — 业务代码无硬编码 schema
- **`WishStatus` / `OrderStatus` 前后端完全一致**（6 值 / 8 值，匹配）
- **WS 重连/心跳逻辑** — 1/2/4s 退避 + 1 分钟冷重试 + 30s 心跳，健壮
- **`dailyCapWarning` 字段在 OpenAPI + 后端 entity + 前端 types 三处已对齐**（**但 P0-2 说了没真填进响应**——类型对但 runtime 漏了）
- **`updateOrderStatus` 200 响应类型** — 之前补的 `body = OrderOutNew` 现在前端能拿到数据
- **42P10 ON CONFLICT 修复** — 全仓 grep 确认只此一处
- **role-swap 阻塞文案** — 已经在用 ORDER_LABEL + 人话原因
- **WISH_REASON_LABELS** — 已生效
- **CLAUDE.md 数据库结构权威约束 + sqlx 字段映射约束 + 部分唯一索引 ON CONFLICT 约束** — 已落地

---

## 建议的修复顺序

### 第一批（一晚上能搞完）

1. P0-1 修 `guest-remark` 路由挂载（1 行 `web::resource`）
2. P0-2 修 `daily_cap_warning` 没填进响应（把变量传到 OrderOutNew 构造里）
3. P0-3 修 `unfreeze_wish_points` 恢复 `users.love_point`（顺手同步 `user_group_points`）
4. P1-6 修 `BREEDER_FINISHED` 那行 SQL 写错的状态名

### 第二批（半天）

5. P1-2 补订单状态 WS 推送（`order_update`）+ 前端订阅
6. P1-1 / P1-4 / P1-5 删死代码（12 个 WS 事件类型 + 4-5 个前端 API 函数 + 60+ 个孤儿 API）
7. P2-1 `feedback.vue` 加载失败给用户提示

### 第三批（看要不要做）

8. P2-2 `todayslist` 要么实现要么删
9. P2-5 全局错误拦截器把后端 errMsg 翻译成人话
10. P1-8 提取 `reject_wish` / `close_wish_internal` 公共逻辑
11. P1-7 确认 `swapRoleCheck` 实际有被调
12. P2-3 改 `as any` 为正类型
13. P2-4 改注释里的"接单人/下单人/订单"
14. P2-6 接入累计投喂真实数据
15. P2-7 前端加 `WishQualityRewarded` WS handler

---

## 已知历史修过的（这次没回退）

- 之前的 SQL 类型转换 bug（`feedback_status_enum → wish_status_enum`）已修
- 之前的 42P10 ON CONFLICT 缺 WHERE 子句 bug 已修
- 之前的 list 列表 counts 跟随 status filter bug 已修
- 之前的 兑换只有接单人能点 bug 后端已修
- 之前的 确认完成新流程（接单人确认）已落地
- 之前的 心愿 reason 改人话展示已生效
- 之前的 wishDetail / feedback 页面分流已落地
- 之前的 role-swap 阻塞文案已落地
- 之前的 文案：接单人/下单人 → 吃货/饲养员、订单 → 清单/心愿池 已落地
