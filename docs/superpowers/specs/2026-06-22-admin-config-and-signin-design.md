# admin 系统配置 + 签到配置完善 设计

> **状态**：待审（设计已通过用户口头批准，待 spec 文档过审）
> **作用域**：may_store 后端 + multi-admin 前端
> **不涉及**：数据库 schema 变更（复用 `global_configs` + `sign_in_records` 现有字段）
> **取代**：早前"每个组配签到奖励"的方案（已统一为 admin 全局配）

---

## 1. 背景与目标

**当前现状**：
- 签到服务（`src/application/sign_in_service.rs`）读 `users.diamond`、写 `users.diamond`、写 `diamond_transactions` 用 `user_id` —— **加的是用户个人钻石**。但产品要求加的是**组共享钻石池**（`association_groups.diamond`）
- `global_configs` 表里没有 `fullTeamBonusAmt` 的配置项；`sign_in_records.full_team_bonus` 字段已预留但代码从不写
- `GET /api/admin/configs` 接口的 OpenAPI 声明返回 `ConfigResponse`（结构体 4 个字段），但实际 handler 返回的是 `Map<String, Value>` 兜底值 —— **文档与实际不一致**
- multi-admin 前端 `ConfigPage` 是占位提示符；`api-paths.ts` 写 `config: '/admin/config'`（单数、缺 `/api` 前缀），跟后端 `/api/admin/configs` 对不上

**用户诉求**（人话版）：
- admin 能在 multi-admin 的「系统配置」页**改 5 个配置项**，改完用户签到立即生效
- 5 个配置项：7 天签到奖励、订单积分百分比、钻石解锁价格、默认足迹容量、**新增**「全组满签奖励」金额
- 用户签到加的是**组钻石**，不是个人钻石
- 全组成员（≥ 2 人）当天都签到时，**最后签到的那位**拿满签奖励（一次、不重复）

---

## 2. 设计决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 签到钻石流向 | 组钻石（`association_groups.diamond`） | 产品要求；与 `wish_quality_reward` 等保持一致 |
| 7 天奖励存储 | 已有 `global_configs.signInRewards7Days`（JSONB 数组） | 已实现、不动 schema |
| 满签金额存储 | 新增 `global_configs.fullTeamBonusAmt`（JSONB 整数） | 与现有配置项保持同一张表 |
| 满签人数门槛 | 写死"组成员 ≥ 2 且都签到" | 用户明确选择硬编码、暂不加 `fullTeamBonusMinMembers` 配置 |
| 满签触发时机 | 实时：最后签到的人触发 | 简单、无需调度任务；与"成员都签到了再发"的直觉一致 |
| 满签重复触发 | 一组一天只发一次 | 走 `sign_in_records.full_team_bonus = TRUE` 判定，幂等 |
| 满签 = 0 | 跳过流程（不写流水、不打标记） | 避免污染审计 |
| 权限 | 不区分角色（AdminToken 放行即改） | 用户决定 |
| OpenAPI 修法 | 改 handler 返回 `ConfigResponse` 结构体（强类型） | 与项目其它接口风格一致；前端 sync 出来的类型才准 |
| multi-admin 路径 | `api-paths.ts` 改 `'/api/admin/configs'`（复数、补 `/api`） | 与 may_store 实际路由一致 |
| multi-admin UI 形态 | 单页 + 卡片分组 + 每卡片独立保存按钮 | 简单、配置项不多、不需要 tab/分页 |
| 缓存 | 不缓存（每次签到读 DB） | 配置变化即时生效、毫秒级查询走索引 |
| 集成测试 | 不写（项目无 DB 基建） | 走单元测试守住纯函数，DB 行为靠手测 |
| 前端测试 | 不写 | 用户明确选择 |

---

## 3. 数据模型

**无 schema 变更**。所有表结构已具备承载本次改动的能力。

### `global_configs` 表（v3.sql L1488）

新增 1 行种子数据（在 v3.sql 注释里加 INSERT 提示）：

| config_key | config_value | category | description |
|---|---|---|---|
| `fullTeamBonusAmt` | `10` | `SIGN_IN` | 全组满签时最后签到用户获得的组钻石数 |

注：实际运行时由 admin 在 multi-admin 配；建表脚本里**只**塞默认值，保证服务冷启动有兜底。

### `sign_in_records` 表（v3.sql L1294）

不动。但本次实现会**写入**之前未用到的字段：

| 字段 | 本次写入场景 |
|---|---|
| `full_team_bonus` | 全组满签且当天未发过时，标 `TRUE`；其它情况 `FALSE` |
| `full_team_bonus_amt` | 同上场景，写入实际发放金额（≥ 1） |

---

## 4. API

### 4.1 后端：sign-in 流程重写

**入口**：`POST /api/groups/{group_id}/sign-in`（或现有等价端点，handler 不动）

**核心改动**（`src/application/sign_in_service.rs::SignService::daily_checkin`）：

```rust
// 1. 现有检查：今日已签到？组信息？连续天数？
// 2. 读 global_configs 拿 7 天奖励（已有 load_sign_rewards）
// 3. 算基础奖励 diamond_reward = calculate_sign_diamonds(consecutive_days, &rewards)
// 4. 【新】写 sign_in_records（先不带 full_team_bonus 标记）
// 5. 【新】加组钻石：UPDATE association_groups SET diamond = diamond + $1 WHERE group_id = $2
// 6. 【新】写 diamond_transactions：
//      group_id=$1, type='EARN', amount=$2,
//      balance_before=($3), balance_after=($3 + $2),
//      biz_type='SIGN_IN', biz_id=sign_id, idempotency_key=format!("sign_in_{}", sign_id)
// 7. 【新】判定"全组满签"：
//      a. 查 SELECT COUNT(*) FROM association_group_members
//         WHERE group_id = $1 AND member_status = 'ACTIVE'
//         → 得到 group_member_count（注意：不是 is_primary；只要是该组 ACTIVE 成员都算）
//      b. 查 SELECT COUNT(DISTINCT user_id) FROM sign_in_records
//         WHERE group_id = $1 AND sign_date = $today
//         → 得到 signed_today_count
//      c. 查 SELECT EXISTS(SELECT 1 FROM sign_in_records
//         WHERE group_id = $1 AND sign_date = $today AND full_team_bonus = TRUE)
//         → 得到 already_paid_today
// 8. 触发条件：group_member_count >= 2
//             AND signed_today_count == group_member_count
//             AND NOT already_paid_today
// 9. 若触发：
//      a. 读 global_configs.fullTeamBonusAmt（缺失/格式错 → 默认 10）
//      b. 若 amt > 0：
//         - UPDATE sign_in_records SET full_team_bonus = TRUE, full_team_bonus_amt = $1
//           WHERE id = $sign_id
//         - UPDATE association_groups SET diamond = diamond + $1 WHERE group_id = $2
//         - INSERT INTO diamond_transactions
//           (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key)
//           (group_id, 'EARN', amt, before_bonus, after_bonus, 'FULL_TEAM_BONUS', sign_id, format!("full_team_bonus_{}", sign_id))
// 10. 事务 COMMIT；返回 DailyCheckinOut { diamond_reward, consecutive_days, total_diamonds, full_team_bonus, full_team_bonus_amt }
```

**并发安全**：所有读改用同一个 `db.begin()` 事务；对 `sign_in_records` 的查询走 `FOR UPDATE` 锁新插入那行（避免"两个用户同时判定都触发"）。已发过的组（`full_team_bonus = TRUE` 存在）由 `already_paid_today` 兜底跳过。

### 4.2 后端：admin/configs 改动

**入口**：`GET /api/admin/configs`、`PATCH /api/admin/configs/{config_key}`

**改动**（`src/api/admin/routes.rs`）：

1. **新增** `ConfigResponse` 实际被使用 —— handler 改成组装结构体返回，不再用 `serde_json::Map`：

   ```rust
   #[derive(Serialize, ToSchema)]
   #[serde(rename_all = "camelCase")]
   pub struct ConfigResponse {
       pub sign_in_rewards_7_days: Vec<i32>,
       pub order_point_percent: i32,
       pub diamond_unlock_cost: i32,
       pub default_footprint_capacity: i32,
       pub full_team_bonus_amt: i32,
   }
   ```

2. **新增** `fullTeamBonusAmt` 加入 PATCH 白名单（范围 0~100，category `SIGN_IN`）：

   ```rust
   ("fullTeamBonusAmt", 0, 100, "SIGN_IN")
   ```

3. **修** GET handler 兜底：默认值从 `serde_json::Map` 改成直接构造 `ConfigResponse` 兜底实例
4. **修** PATCH handler 的 `signInRewards7Days` 成功响应：从 `serde_json::json!` 改成 `ApiResponse::success(ConfigResponse { ... })` 一致化（其它配置项已是这样）
5. **OpenAPI 注解** 与实际返回对齐（已对齐因为改的是结构体）

**未改**：

- 路由注册（已存在）
- `audit_logs` 写入（已实现）
- AdminToken 中间件（已实现）

### 4.3 前端：multi-admin ConfigPage 实现

**入口**：`/store/config`（路由已存在）

**文件改动**：

| 文件 | 改动 |
|---|---|
| `src/constants/api-paths.ts` | `config: '/admin/config'` → `config: '/api/admin/configs'` |
| `src/features/store/config/ConfigPage.tsx` | 占位 → 真实页面 |
| `src/features/store/config/hooks.ts` | **新建**：`useConfigs`（拉）、`useUpdateConfig`（改） |
| `src/features/store/config/types.ts` | **新建**：从 `@/api/generated/store` 引用类型 |

**UI 形态**（单页，3 卡片）：

```text
┌─ 系统配置（/store/config）────────────────────────────────┐
│                                                           │
│ ┌─ 签到配置 ──────────────────────────────┐               │
│ │  7 天签到奖励（7 个数字，1~100）         │               │
│ │  [5][6][7][8][9][10][20]   [保存]        │               │
│ │  全组满签奖励金额（0~100）               │               │
│ │  [10]                       [保存]       │               │
│ └────────────────────────────────────────┘               │
│                                                           │
│ ┌─ 订单配置 ──────────────────────────────┐               │
│ │  订单积分百分比（1~200）                 │               │
│ │  [100]                      [保存]       │               │
│ └────────────────────────────────────────┘               │
│                                                           │
│ ┌─ 通用配置 ──────────────────────────────┐               │
│ │  钻石解锁价格（10~10000）                │               │
│ │  [100]                      [保存]       │               │
│ │  默认足迹容量（10~1000）                 │               │
│ │  [50]                       [保存]       │               │
│ └────────────────────────────────────────┘               │
└───────────────────────────────────────────────────────────┘
```

每个字段独立保存（保存按钮 disabled 当值未变 / 正在提交），错误信息展示在字段下方。

**关键复用**：

- `useUpdateConfig` 调用 `PATCH /api/admin/configs/{key}`，body `{ value: <typed value> }`
- 7 天数组用 `InputNumber` 7 个横排，每个 `min={1} max={100}`
- 其它整数用单个 `InputNumber`
- 加载态用 antd `Skeleton`；错误用 antd `App.useApp().message`
- 复用 `AppShell` 已有的 `Card` 容器（如有）

---

## 5. 文件改动清单

### may_store

| 文件 | 改动 |
|---|---|
| `src/application/sign_in_service.rs` | 改 `daily_checkin`：换组钻石 + 加满签判定 + 扩展返回结构 |
| `src/api/admin/routes.rs` | 改 `get_config` 返回 `ConfigResponse`；`update_config` 加 `fullTeamBonusAmt` 白名单；统一响应 |
| `src/domain/sign_in/entities.rs` | （可能）扩 `DailyCheckinOut` 加 `full_team_bonus` / `full_team_bonus_amt` 字段 |
| `src/v3.sql` | **仅注释**：在 `global_configs` 表附近加 INSERT `fullTeamBonusAmt` 默认值的提示（不破坏 DROP/重建） |

**不动**：

- `src/domain/user/entities.rs`（之前已删除 sign_reward 字段，本次不变）
- 任何 DDL

### multi-admin

| 文件 | 改动 |
|---|---|
| `src/constants/api-paths.ts` | 修 `config` 路径 |
| `src/features/store/config/ConfigPage.tsx` | 占位 → 实现 |
| `src/features/store/config/hooks.ts` | **新建** |
| `src/features/store/config/types.ts` | **新建** |

**不动**：

- 路由（已存在）
- 其它 feature 页（不在范围）

---

## 6. 测试

### 6.1 后端单元测试（`cargo test`）

**`src/api/admin/routes.rs` 末尾**（新建 `mod tests`）：

| 用例 | 期望 |
|---|---|
| `validate_sign_in_rewards_7_elements` | 7 位合法值通过 |
| `validate_sign_in_rewards_6_elements` | 6 位拒 |
| `validate_sign_in_rewards_8_elements` | 8 位拒 |
| `validate_sign_in_rewards_0_element` | 空数组拒 |
| `validate_sign_in_rewards_element_0` | 元素 0 拒 |
| `validate_sign_in_rewards_element_101` | 元素 101 拒 |
| `validate_sign_in_rewards_element_string` | 元素非整数拒 |
| `validate_int_config_in_range` | 在范围内通过 |
| `validate_int_config_below_min` | 低于下限拒 |
| `validate_int_config_above_max` | 高于上限拒 |
| `validate_int_config_string_value` | 非整数拒 |
| `validate_unknown_key` | 未知 key 拒 |

实现方式：把 `update_config` 里的校验逻辑**抽出纯函数** `validate_config(key, value) -> Result<(), CustomError>`，单测直接调。

**`src/application/sign_in_service.rs` 末尾**（扩充已有 `mod tests`）：

| 用例 | 期望 |
|---|---|
| `is_full_team_signed_both_signed` | 2 人组、2 行 sign_in_records → true |
| `is_full_team_signed_one_signed` | 2 人组、1 行 → false |
| `is_full_team_signed_single_member_group` | 1 人组、1 行 → false（产品规则） |
| `is_full_team_signed_already_paid_today` | 同组已有 `full_team_bonus=TRUE` → false |
| `bonus_zero_skips_flow` | fullTeamBonusAmt=0 → 不写流水、不打标记 |

实现方式：抽 `is_full_team_signed(group_id, today) -> bool` 与 `apply_full_team_bonus(...)` 两个纯函数（参数化 DB 调用），单测直接调。

**已有**（不动）：

- `calculate_sign_diamonds` 7 天循环、空数组、自定义长度 —— 已覆盖

### 6.2 不测

- DB 事务并发（需要真实 Postgres，**项目无基建**）—— 文档留 TODO
- HTTP 接口端到端（理由同上）—— 手测
- 前端（用户决定不加）

### 6.3 跑

```bash
cd /home/peter/project/may_store
cargo test
```

应全绿。新增约 17 个用例。

---

## 7. 验收清单

- [ ] `cargo check` 通过
- [ ] `cargo build --release` 通过
- [ ] `cargo test` 全绿（含新增 17 个用例）
- [ ] `global_configs` 表里有 `fullTeamBonusAmt` 默认值（10）
- [ ] 用户签到：`association_groups.diamond` 增加、`users.diamond` **不变**
- [ ] 写 `diamond_transactions` 时用 `group_id` 不用 `user_id`、`biz_type='SIGN_IN'`
- [ ] 2 人组当天都签到：最后签到者写 `sign_in_records.full_team_bonus = TRUE`、再多加 1 笔组钻石、写 1 笔 `biz_type='FULL_TEAM_BONUS'` 流水
- [ ] 1 人组签到：写正常记录、**不**写满签标记、**不**发满签奖励
- [ ] 已发过满签的组、新成员签到：不写满签标记、**不**重复发奖励
- [ ] `fullTeamBonusAmt=0` 时：判定可能通过、但**不**写流水、**不**打标记
- [ ] `GET /api/admin/configs` 返回 `ConfigResponse` 结构体（5 个字段），OpenAPI 文档与之一致
- [ ] `PATCH /api/admin/configs/fullTeamBonusAmt` 接受 0~100 整数、超出范围 400
- [ ] `PATCH` 任意配置项都写 `audit_logs`（operator_id/operator_type/action_type/target_type/detail）
- [ ] multi-admin `api-paths.ts` 的 `config` 路径指向 `/api/admin/configs`
- [ ] multi-admin `ConfigPage` 拉得到 5 个配置项的当前值
- [ ] multi-admin `ConfigPage` 编辑后保存，刷新页面能拿到新值
- [ ] multi-admin `ConfigPage` 字段超范围时显示后端错误信息、**不**关闭页面
- [ ] Swagger UI 上 `ConfigResponse` 的 schema 与 may_store 实际返回一致

---

## 8. 不在本次范围

- 角色权限细分（SUPER_ADMIN/OPS/RISK_REVIEWER）—— 暂不做
- 满签人数可配（`fullTeamBonusMinMembers`）—— 暂不写死
- 调度任务（0 点批量补发满签奖励）—— 暂不做
- 集成测试 / DB fixture —— 暂不搭基建
- 前端单元测试 —— 不写
- multi-admin 其它占位页（Users/Groups/Orders/AuditLogs）—— 不动
- `global_configs` 表已有但未白名单化的其它配置项 —— 不动
- 国际化（i18n）—— 不动
- 满签历史/审计（谁拿过满签、什么时候）—— 走 `sign_in_records` 现有字段，**不**新建审计表
