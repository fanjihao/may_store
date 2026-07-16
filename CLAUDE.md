# Claude 协作规范

> **本文件优先级最高**：本文件中的规则覆盖 Claude Code 默认行为。当默认行为与本文件冲突时，以本文件为准。
>
> 本项目所有需求来源是一位**非技术决策者** —— Claude 必须在动手改任何文件、写任何代码、跑任何命令之前，先按本规范处理。

## 1. 用户画像（人机协作前提）

- **不是工程师**：不懂技术术语、库、协议、字段命名、JWT、async、ORM、CRUD、DDL 这类词对他无意义。
- **需求是日常语言**：可能是模糊、跳跃、情绪化、甚至前后矛盾的"我想要什么"，而不是"用什么技术怎么实现"。
- **沟通风格直接**：可能带情绪、可能骂人、可能一次说完就当你懂。**不是为难，是表达习惯**。

## 2. 强制处理流程（每条需求 MUST）

收到任何需求，按顺序执行以下步骤，**前一步没得到用户确认，不得进入下一步**。

### 2.1 先理解，后动手（绝不动手前先做这些）

- [ ] 这个需求要解决**什么问题**？（用户视角，不是技术视角）
- [ ] 涉及哪些**角色**？（谁用、谁看、谁配置、谁审批）
- [ ] 关键**流程**和**边界**是什么？（没说但显然应该有的：异常、空状态、权限、移动端 vs 桌面端等）
- [ ] 数据**怎么走**？用户**看到什么**？**什么算完成**？

### 2.2 把理解说出来再确认（MANDATORY，零例外）

- 用大白话复述"我理解你要的是这个"，必要时配示意图 / 流程图 / 字段示例。
- **禁止默认"用户说的就是字面意思"**。
- **禁止用技术名词解释技术名词**。
- 不动手写代码、不改文件、不跑命令，**先得到用户确认**才进入下一步。

### 2.3 不明确就问，不要猜

- 需求有歧义 / 缺条件 / 多种合理解读时，**主动列出 2~4 个可能方向让用户选**，而不是自己拍板。
- **禁止"反问'你到底想要什么'"** —— 给出我的理解让用户纠正。

### 2.4 不要被脾气带偏

- 用户语气不好时，关注**问题本身**，不抬杠、不反驳、不解释技术限制。
- 技术限制要**翻译成产品语言**（不写"数据库不支持"，写"这个目前做不到的原因是 X，我们有 A、B 两个变通方案"）。

### 2.5 方案要给完整路径

- 不只说"改一个文件"，要把"产品上要做什么、数据怎么走、用户看到什么、什么算完成"讲清楚。

### 2.6 改动完成后用人话汇报

- 告诉用户**"现在用户能做什么了"**，而不是"我改了 src/foo/bar.rs 第 42 行"。

## 3. 禁用行为（MUST NOT，零容忍）

- ❌ 上来就问技术细节（库、协议、字段名）—— 这些由我们自己定。
- ❌ 把用户的话当技术需求直接实现。
- ❌ 因为需求模糊就反问"你到底想要什么"。
- ❌ 解释技术概念（JWT、async、ORM、DDL、CRUD 是什么 —— 用户不在乎）。
- ❌ 在用户确认理解前动手改任何文件 / 写任何代码 / 跑任何命令。
- ❌ 用"我建议这样实现"绕过产品理解环节。

## 4. 触发条件

以下场景**必须**按本规范处理：

- 收到任何需求（无论多小、无论多"显然"）。
- 用户说"做一下"、"加一下"、"改一下"、"搞个 XX"、"这个不对，处理下"。
- 用户给一段描述但没明确说什么算完成。
- 用户上传截图 / 录屏 / 参考图说"做成这样"。

## 5. 违反处理

如果 Claude 没有按本规范处理（比如直接动手写代码了、没确认就改了、用了技术词解释技术词），用户可以 / 应该：

- **直接打断**："先别动，跟我确认下你的理解"或"按规范来，重来一遍"。
- **让 Claude 退回到 §2.1 步骤重来**。

## 6. 汇报模板（改完说什么）

改完后用以下结构汇报（用人话，不要堆技术细节）：

1. **现在用户能做什么了**（产品视角）。
2. **看到了什么 / 触发什么**（用户视角的操作路径）。
3. **什么算完成 / 什么算没完成**（验收标准）。
4. **如果有限制，列在最后**，翻译成产品语言（"这个目前做不到，原因是 X"），不要藏在技术细节里。

## 数据库结构（v3.sql）权威约束

- `src/v3.sql` 是项目数据库结构的**唯一权威来源**（全量建表脚本，带完整类型 / 索引 / 注释）。
- 凡是涉及数据库结构变更的工作（新建表、加列、改类型、改索引、新增 enum、加 / 改函数），**必须同步更新 v3.sql**，把变更直接落到这个文件里。
- 新增接口、改 SQL 查询时：
  - 如果只用 v3.sql 已有的表和列，不需要动 v3.sql。
  - 如果发现代码里要用的表 / 列在 v3.sql 里没有，**先补 v3.sql，再写代码**，不允许出现"代码已经查某列但 v3.sql 里没定义"的情况。
- 代码里出现 `CREATE TABLE`、`ALTER TABLE`、`CREATE TYPE`、`CREATE INDEX` 这种 DDL 语句时，需要确认：这条 DDL 在 v3.sql 里有没有对应？没有就补上，不能让 DDL 只活在代码注释或迁移脚本里。
- 接手 / 复盘时：对一遍"代码里所有 FROM / JOIN / INSERT / UPDATE 用到的表和列" vs "v3.sql 实际定义的表和列"，有差异就要修复 v3.sql。

## sqlx 字段映射一致性约束

- Rust 端使用 `sqlx::query_as::<_, SomeStruct>` 反序列化 DB 行时，**字段名必须能在 SQL 查询结果中找到对应列**。
- 常见踩坑：结构体上 `#[sqlx(rename = "tag_id")]` 会让 sqlx 去找名为 `tag_id` 的列；但如果 SQL 里写的是 `SELECT tag_id AS id, ...`，Row 里只有 `id` 这一列，**rename 目标找不到 → 运行时 `ColumnNotFound("tag_id")` → 报 "查询字段不存在"**。
- **禁止**在结构体字段上加 `#[sqlx(rename)]` 然后又在 SQL 里 `AS` 成另一个名字——两边必须一致。
- 已有约束：
  - 本项目用 `scripts/check_query_as.py` 扫描所有 `query_as` 调用，自动检测这种不一致。
  - 已集成到 `cargo test` —— 改完 Rust 代码必须 `cargo test` 通过才能提交。
  - 集成测试位置：`tests/sqlx_rename_consistency.rs`。

## PostgreSQL 部分唯一索引 + ON CONFLICT 强制对齐

- `v3.sql` 里凡是 `idempotency_key` 字段上的**幂等键唯一索引**都是**部分唯一索引**：

  ```sql
  CREATE UNIQUE INDEX idx_lpt_idempotency  ON love_point_transactions(idempotency_key)  WHERE idempotency_key IS NOT NULL;
  CREATE UNIQUE INDEX idx_get_idempotency  ON group_exp_transactions(idempotency_key)    WHERE idempotency_key IS NOT NULL;
  CREATE UNIQUE INDEX idx_dt_idempotency   ON diamond_transactions(idempotency_key)      WHERE idempotency_key IS NOT NULL;
  ```

- 用部分索引的原因：`idempotency_key` 是可空列，PG 的标准 `UNIQUE` 约束对 NULL 视为互不重复，多个 NULL 不冲突；用 `WHERE ... IS NOT NULL` 才能保证「非 NULL 时全局唯一」。
- **后果**：代码里写 `INSERT ... ON CONFLICT (idempotency_key) DO NOTHING` 会**直接报错 42P10 `no_unique_or_exclusion_constraint`**——PG 要求 ON CONFLICT 必须带上与索引**完全一致**的 WHERE 谓词，否则匹配不上索引。
- **正确写法**（**必须**带 WHERE 子句）：

  ```sql
  INSERT INTO group_exp_transactions (...) VALUES (...)
  ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING
  ```

- **禁止**的写法：

  ```sql
  -- 缺 WHERE,会 42P10
  ON CONFLICT (idempotency_key) DO NOTHING
  ```

- 适用范围：v3.sql 里所有用 `WHERE idempotency_key IS NOT NULL` 创建的部分唯一索引（上面列了 3 张表）。
- 改代码时：每次新加 `ON CONFLICT (idempotency_key)` 前，对照 v3.sql 把对应索引的 WHERE 子句原样抄过来。

## 积分变动必须同步 user_group_points

- 项目里有**两套积分余额**：
  - `users.love_point` —— 全局余额（**真正在变动的字段**，前端个人中心 / 订单完成 push 都基于它）
  - `user_group_points.available_love_point` + `frozen_love_point` —— 组内可用 / 冻结
- 1v1 模型下两者应该一致，但**积分消费类路径写完 `users.love_point` 之后必须同步 `user_group_points`**，否则积分商城（getPointsBalance 读 `user_group_points`）会显示陈旧数据。
- **必须同步 `user_group_points` 的 4 个更新点**：
  1. **订单完成**（CONFIRMED_COMPLETED / CONFIRMED_INCOMPLETE）→ `application/order_service.rs` `update_order_status` 内 UPDATE users 后
  2. **订单评分**（reward / penalty）→ `application/order_service.rs` 评分事务内 UPDATE users 后
  3. **心愿兑换**（FREEZE：可用减少 + 冻结增加）→ `application/wish_service.rs` `select_wish` 内 UPDATE users 后
  4. **新增路径时**：任何新加的改 `users.love_point` 的代码，**必须**紧接着 UPSERT `user_group_points`（用 `INSERT ... ON CONFLICT (user_id, group_id) DO UPDATE` 模式）
- 已知未修的尾巴：`unfreeze_wish_points`（拒绝心愿/心愿过期调用）~~只写 UNFREEZE 流水、不实际恢复 `users.love_point`~~ **已于 2026-07-15 修**，函数体改成事务：写流水 + `UPDATE users SET love_point = love_point + frozen_amount` + UPSERT user_group_points。

## 改动 Rust 代码后必须跑的检查

- 改完 Rust 代码 → 在 `cargo check` 通过之后，**必须**再跑：、

  ```bash
  CARGO_BUILD_JOBS=2 cargo test --test sqlx_rename_consistency
  ```
  
- 这是 `cargo test` 默认会跑的集成测试之一（如果只改 Rust 代码也可以直接 `cargo test`）。
- 如果测试失败，提示"某字段 rename 在 SQL 中找不到对应列"——按错误信息修代码（要么删 rename 让 sqlx 用 Rust 字段名找，要么改 SQL 别 `AS`）。
- 这一条是**强制的**——任何改了 Rust 代码的 commit 都必须通过该测试。

## 7. 相关项目位置

- **前端项目 `wx-store`**：与本仓（`may_store`）同级，路径 `../wx-store`（绝对路径 `/home/peter/project/wx-store`）。
- 当用户提到"前端" / "页面" / "UI" / "stash 页 / 餐厅 / 厨房信息展示"等视觉/交互相关需求时，**优先想到去 `../wx-store` 改前端代码**；本仓（`may_store`）只负责接口与数据。
- 改前端时不要顺手改本仓的 API；改本仓 API 时不要顺手改前端。两边需要同步时，先跟用户确认是哪一边。

## 8. 已整改记录（2026-07-15 review）

详细 review 报告见 `docs/system-review-2026-07-15.md`。下面是已修的项：

### 已修的 P0（真 bug）
- ✅ P0-1 删 `update_guest_remark`（功能与 `createOrder.vue` 备注字段重复，已删路由 + handler + struct）
- ✅ P0-2 `daily_cap_warning` 填进响应（已完成，之前对话里）
- ✅ P0-3 `unfreeze_wish_points` 恢复 `users.love_point`（包事务，加 UPDATE users + UPSERT user_group_points）

### 已修的 P1（契约/接缝）
- ✅ P1-1 删 12+ 死代码 WS 事件类型（`OrderRiskDetected` / `WishCreated` / `LovePoint*` / `Group*` / `DiamondEarned` / `RoleSwapped` / `OrderReviewed` / `FootprintPublished` / `WishFulfilled` / `DiamondConsumed` / `PointChanged`）
- ✅ P1-2 订单状态变更 WS 推送 + 前端 `order.vue` / `orderDetail.vue` 订阅
- ✅ P1-5 删 7 个死代码前端 API（`getPointsTransactions` / `getDiamondsBalance` / `getDiamondsTransactions` / `getGroupExp` / `getExpTransactions` / `getWishCheckins` / `pendingFulfillment`，后端 + 前端同步删）
- ✅ P1-6 修 `today_todos.rs` SQL 状态名（`BREEDER_FINISHED` 不存在于 enum，改成 `PRODUCTION_COMPLETED`）
- ✅ P1-7 home.vue 角色切换按钮 `:class` 视觉锁 → `:disabled` 真禁用
- ✅ P1-8 抽 `reject_wish` / `close_wish_internal` 公共逻辑（→ `reject_or_close_wish`）

### 已修的 P2（体验/规范）
- ✅ P2-1 `feedback.vue` 加载失败给 toast
- ✅ P2-3 `wishDetail.vue` 改 `: any` → 正类型（`WishOutWithNegotiations` / `NegotiationItem`）
- ✅ P2-4 注释里"接单人/下单人"清理
- ✅ P2-5 全局错误拦截器（`silenceError` 工具 + CLAUDE.md 规范文档）

### 不做 / 不可做
- ❌ P1-3 `group_diamond_change` 订单路径推送（用户说"目前只有签到解锁容量才会有钻石变化"，不做）
- ➖ P1-4 P1-7 已是合法实现（不是 review 误判的死代码，文档已修正）
- ➖ P2-2 `todayslist.vue` 用通用 `getOrders` + 前端分桶补上了
- ❌ P2-6 `累计投喂 0 次` 写死（placeholder，等后端字段接入）
- ❌ P2-7 `WishQualityRewarded` 前端订阅（事件类型已删、后端 admin 接口不发 WS，前置条件缺失）
