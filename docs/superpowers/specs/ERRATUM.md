# ERRATUM — FSD / API Doc 与实现差异

> **更新时间**: 2026-06-13
> **审计来源**: 4 源(FSD/API doc/代码/Swagger)交叉对照
> **权威来源**: 当文档 vs 代码不一致时,以**代码 + OpenAPI 生成的 Swagger 为准**(已经过 P0/P1 对齐改造)。本 ERRATUM 列出文档侧待修正项,供下次文档更新时一并修订。

---

## A. FSD 文档错误(需修正 FSD.latest.md)

### A.1 §11.20 `/api/admin/audit-logs` 方法误标
- API doc part3:1254 写为 `POST`,FSD 同段错标
- **正确**: `GET /api/admin/audit-logs?cursor=&limit=&...`
- 代码: [src/api/admin/routes.rs:43](../../src/api/admin/routes.rs#L43) `web::get().to(get_audit_logs)`

### A.2 §3653 纪念日 PATCH/DELETE 路径
- FSD 描述: `PATCH /api/groups/{group_id}/memorial-days` (无 inner id)
- **正确**: `PATCH /api/groups/{group_id}/memorial-days/{id}` (REST 单条修改必须带 id)
- 代码: [src/api/memorial_days/routes.rs:25-27](../../src/api/memorial_days/routes.rs#L25)

### A.3 §3517 标签 PATCH/DELETE 路径
- FSD 描述: `PATCH/DELETE /api/groups/{group_id}/tags` (无 inner id)
- **正确**: `PATCH/DELETE /api/groups/{group_id}/tags/{tag_id}`
- 代码: [src/api/tags/routes.rs:25-26](../../src/api/tags/routes.rs#L25)

### A.4 §3549 食材 (ingredients) 端点未实现
- FSD 列出 4 个端点 (POST/GET/PATCH/DELETE `/api/groups/{group_id}/ingredients`)
- **现状**: 代码 `src/api/` 下没有 `ingredients` 模块
- **建议**: 在 FSD §3549 顶部加状态徽章 `状态: 未实现 v1.0,排期 vNext`
- 临时方案: 食材可暂时通过菜品创建时的 `ingredients` 数组字段携带([src/api/foods/routes.rs](../../src/api/foods/routes.rs) FoodIngredient)

### A.5 §3716 / §3725-3726 做客邀请命名空间
- FSD 描述: `/api/groups/{group_id}/guest-invitations`、`/api/admin/groups/{group_id}/guest-invitations/{id}/revoke`
- **实际代码使用**: `/api/kitchens/invitations/{invite_code}` 命名空间
- **决策**: 以代码为准 —— `kitchens` 语义更清晰(主人家厨房视角),修 FSD 章节命名

### A.6 客服工单 (`/api/support-tickets`) 章节缺失
- 代码已实现 3 个端点: [src/api/support_tickets/routes.rs](../../src/api/support_tickets/routes.rs) (create/list/get)
- **建议**: 在 FSD 加 §15.x 章节描述 SupportTickets 数据模型与流程

---

## B. API 文档错误(需修正 docs/superpowers/specs/2026-06-03-api-design-part*.md)

### B.1 part3:1254 `/api/admin/audit-logs` 方法误标
同 A.1。修为 `GET`。

### B.2 part1:96 / part1:117 WechatLoginResponse 假分支
- API doc 把同一 response 画成"新用户"/"老用户"两套 schema
- **实际代码**: 只有一个 `WechatLoginResponse { user_id, nickname, avatar_url, access_token, refresh_token, is_new_user }`,通过 `is_new_user` 字段标识
- **修正**: 合并为单 schema,在描述里说明 `is_new_user=true` 时 `nickname`/`avatar_url` 通常为空

### B.3 API doc 仅覆盖 5 个模块,缺一半
- 已覆盖: auth / users / groups / foods / orders(part1)+ wishes / economy / sign_in / footprints / achievement(part2)+ notifications / upload / ws / admin / dashboard(part3)
- 缺: memorial_days / tags / ingredients(未实现)/ food_marks / footprint_groups / support_tickets / kitchens
- **建议**: 新增 `part4-additional-modules.md` 补充以上模块

### B.4 part2:328 WishRejectInput
- API doc 描述与实际 schema 字段可能有差异(未逐字段比对,留待详细 review)
- **建议**: 用 Swagger UI 的 schemas 视图与 API doc 逐字段核对

---

## C. 代码侧的命名约定(已在 P0/P1 改造中统一,文档需跟进)

### C.1 路径参数命名
- **统一**: `{order_id}` / `{wish_id}` / `{food_id}` / `{group_id}` / `{footprint_group_id}` / `{config_key}`
- **历史**: orders/wishes 模块曾用 `{id}` 单字,现已全部改为 `{order_id}`/`{wish_id}`
- 文档应使用相同命名

### C.2 鉴权方案
- **统一**: `bearer_auth` (HTTP Bearer JWT)
- **历史**: 部分 handler 曾标 `cookie_auth`(从未实际生效),已全局替换
- Swagger UI 已正确显示 Authorize 按钮(从 [src/openapi.rs SecurityAddon](../../src/openapi.rs) 注入)

### C.3 Admin 鉴权
- Swagger 显示 `bearer_auth` 但 admin 端点实际还要求 `admin_users` 表中 ACTIVE 记录
- 详见 [src/middlewares/admin_auth.rs](../../src/middlewares/admin_auth.rs)
- 文档可在管理员端点描述里补一行 "需 admin_users 表记录"

### C.4 配置接口
- **统一**: `GET /api/admin/configs` (列表) + `PATCH /api/admin/configs/{config_key}` (单条)
- **历史**: 曾用 `GET/PUT /api/admin/config`(单数,无 path param)—— 已废弃
- 见 [src/api/admin/routes.rs:26-27](../../src/api/admin/routes.rs#L26)

### C.5 Dashboard 后台端点鉴权
- `/api/admin/dashboard` 和 `/api/admin/dashboard/trends` 现已用 `AdminToken` 提取器
- 不再依赖不可信的 `UserRole::Admin`(组内角色)
- 详见 [src/api/dashboard/routes.rs:225, 393](../../src/api/dashboard/routes.rs#L225)

### C.6 Foods CRUD 已补齐
- 6 个端点: POST/GET list/GET single/PATCH/DELETE/POST hide
- 模块: [src/api/foods/](../../src/api/foods/)
- schema 已在 v3.sql 加 3 列(description / images / tags JSONB)

### C.7 JWT 标准化
- Claims 形状: `{ sub, jti, iat, nbf, exp, iss, typ }`
- 算法锁定 HS256,iss 强制校验
- Access TTL 2h,Refresh TTL 30d,refresh rotation 自动黑名单旧 jti
- 详见 [src/middlewares/jwt.rs](../../src/middlewares/jwt.rs)

---

## D. Swagger UI 已知限制

### D.1 WS upgrade endpoint 不在 Swagger 中
- WS 跑在独立端口 9832,utoipa 不支持描述 WS 协议
- Swagger 只展示 `/ws/info` 和 `/ws/status` 两个 HTTP 状态端点
- WS 消息格式见 FSD §1045-1143 / API doc part3:464-651

### D.2 Tag 分组
- 已在 [src/openapi.rs](../../src/openapi.rs) tags() 块中声明完整列表
- 包括: 认证、用户、双人组、菜品、菜品标记 (§24.6)、菜品标签 (§24.4)、纪念日 (§24.9)、足迹分组 (§24.10)、客服工单、做客厨房、订单、评分、心愿、经济查询、签到、足迹、成就、通知、文件上传(七牛直传)、后台管理、数据看板、WebSocket

---

## E. 验证清单

部署后用以下方式验证 4 源一致:

1. **Swagger UI**: 访问 `/swagger/index.html`,Authorize → 贴 JWT → 逐 tag 测试关键接口
2. **OpenAPI JSON**: 拉 `/swagger/openapi.json`,对照 FSD 端点表
3. **重新跑 audit**: 用本仓库的 4 源 inventory 比对脚本(见对话记录),确认零差异
4. **cargo check + clippy**: 0 错误,改动文件 0 新增告警
