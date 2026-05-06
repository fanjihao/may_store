# 心愿菜单 —— 从0开发

---

## 1️⃣ 总体架构设计

### 1.1 分层架构

- **API 层**：RESTful/GraphQL/WebSocket，协议适配、参数校验、认证授权。
- **应用层**：业务用例编排、事务、幂等、领域服务、事件发布。
- **领域层**：核心业务逻辑（如订单状态机、经济联动、成就规则）、聚合根、实体、值对象、领域服务。
- **事件层**：事件定义、事件发布、事件消费（Worker），实现解耦与异步。
- **基础设施层**：数据库、缓存、消息队列、外部服务适配器。

### 1.2 模块划分（全量）

- **user**：用户、认证、分组（group/association）、权限、黑名单
- **order**：订单（任务）、状态机、评价、催办、附件
- **economy**：point/diamond 体系、流水、联动机制、排行榜
- **footprint**：足迹、容量、状态管理、地理位置、图片
- **achievement**：成就、规则、事件驱动、成就墙
- **event**：事件定义、事件日志、Worker、幂等
- **wish**：心愿、心愿池、心愿兑换
- **sign_in**：签到、连续签到、签到奖励
- **notification**：消息推送、系统通知、未读数
- **upload**：文件/图片上传、资源管理
- **admin**：后台管理、运营配置、数据统计
- **utils/middleware**：通用工具、日志、鉴权、限流、审计

> 说明：如需扩展活动、AI推荐、第三方集成等，可独立新模块。

### 1.3 关键设计修复与原则

- **经济系统割裂** → point/diamond 严格分离，所有变动走流水表，联动机制事件驱动。
- **同步耦合** → 业务解耦，成就、经济、足迹等均通过事件流转。
- **并发安全** → 乐观锁/条件更新/唯一约束，防止重复操作。
- **扩展性** → 规则、配置、扩展字段（JSONB），支持新玩法。
- **审计与幂等** → 关键表均有幂等key、软删除、扩展字段、操作人。

---

## 2️⃣ 主要业务模块与核心流程

### 2.1 用户与分组（user/association_groups）

- 用户注册、登录、分组邀请、黑名单、权限管理
- 角色切换：接单人下单人互换，必须无可执行订单（待接单进行中待确认等状态）；并且必须是同一组内用户；并且24h内只能切换一次；切换事件驱动，触发成就/通知等联动
- 组拥有等级概念，等级越高可添加菜品、标签数量等就越多。每升一级**容量+n**，可使用**钻石扩容**

### 2.2 菜品系统（food、foodtag、ingredient）

- 菜品是组级资源，各个组维护各自的菜品、标签、食材库
- 菜品由**组内接单人**维护，包含名称、图片、标签、食材、步骤等信息

### 2.3 订单系统（order）

- **状态机**：
    Created → Accepted → Production complete →  Confirmation complete；
    Created → Accepted → Production complete →  Confirmation incomplete；
    Created → Rejected；
    Ccreated → Cancelled；
    Created/Accepted → Timeout;

- **并发安全**：乐观锁/条件更新防止重复接单
- **评价体系**：订单完成后可评价
- **催办/附件**：支持催办、上传附件
- **生命周期**：创建-待接单状态、撤回-已撤回状态、接单-进行中状态、接单人完成-等待确认状态、完成-下单人确认完成、未完成-下单人确认未完成、超时-已超时状态

### 2.4 经济系统（economy）

- **双货币模型**  
  - Point：用户间付出衡量，仅用于心愿兑换、排行榜、成就
  - Diamond：组资源，仅用于扩容、能力解锁，归属 group
- **边界与联动**  
  - 严格分离用途，禁止 point 直接兑换 diamond
  - 行为 → point，优质行为（如好评、连续互动）→ diamond
- **流水机制**  
  - 所有变动必须写入 point_transactions/diamond_flow，余额通过流水聚合
  - 禁止直接 update 余额
- **并发安全**  
  - 变动用事务+条件更新，防止超发/重复发放

### 2.5 足迹系统（footprint）

- **容量管理**  
  - group 级别容量，diamond 扣除可扩容
- **状态管理**  
  - draft/published/deleted，软删除
- **与订单关联**  
  - 订单完成可自动发布足迹（事件驱动）

### 2.6 成就系统（achievement）

- **事件驱动**：监听event_log，异步计算
- **规则配置化**  
  - 支持新成就规则热插拔，规则存表/配置文件
- **成就墙**：用户/组成就展示

### 2.7 心愿系统（wish）

- 心愿创建、心愿池、兑换、心愿完成事件
- 心愿完成事件触发成就、经济联动
- 心愿由

### 2.8 签到系统（sign_in）

- 签到、连续签到奖励、签到事件
- 签到奖励通过事件发放 point/diamond，支持成就联动
- 签到事件可用于活跃度分析、用户留存等后续功能扩展
- 签到系统设计需考虑时区问题，确保用户在本地时间的连续签到逻辑正确实现
- 签到奖励发放需幂等，防止重复发放导致经济系统混乱
- 组员a签到，组钻石增加；组员b签到，组钻石增加；当两人当天均以签到，组钻石再增加（激励组内互动）

### 2.9 通知/消息系统（notification）

- 系统通知、订单/心愿/成就等事件推送、未读数

### 2.10 文件上传（upload）

- 图片/附件上传、资源管理

### 2.11 后台管理（admin）

- 运营配置、数据统计、权限管理

---

## 3️⃣ 数据库设计（详细表结构建议）

> 说明：所有表建议补充 `idempotency_key`（幂等）、`is_deleted`（软删除）、`extra`（JSONB扩展）、`created_by/updated_by`（操作人）、`created_at/updated_at`（审计），并根据业务补充唯一约束、索引。

### 3.1 users

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 用户ID         | PK, UNIQUE        |
| group_id         | BIGINT         | 所属组         | FK, INDEX         |
| nickname         | VARCHAR        | 昵称           |                   |
| avatar_url       | VARCHAR        | 头像           |                   |
| email            | VARCHAR        | 邮箱           | UNIQUE, 可空      |
| phone            | VARCHAR        | 手机号         | UNIQUE, 可空      |
| status           | VARCHAR        | 状态           | INDEX             |
| last_login_at    | TIMESTAMP      | 最后登录时间   |                   |
| is_active        | BOOLEAN        | 是否有效       | 默认 true         |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |
| updated_at       | TIMESTAMP      | 更新时间       |                   |
| created_by       | BIGINT         | 创建人         | 可空              |
| updated_by       | BIGINT         | 更新人         | 可空              |

### 3.2 association_groups

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 组ID           | PK, UNIQUE        |
| name             | VARCHAR        | 组名           |                   |
| invite_code      | VARCHAR        | 邀请码         | UNIQUE, 可空      |
| diamond          | BIGINT         | 当前钻石       |                   |
| footprint_capacity | INT          | 足迹容量       |                   |
| settings         | JSONB          | 组配置         |                   |
| is_active        | BOOLEAN        | 是否有效       | 默认 true         |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |
| updated_at       | TIMESTAMP      | 更新时间       |                   |
| created_by       | BIGINT         | 创建人         | 可空              |
| updated_by       | BIGINT         | 更新人         | 可空              |

### 3.3 orders

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 订单ID         | PK, UNIQUE        |
| group_id         | BIGINT         | 所属组         | FK, INDEX         |
| creator_id       | BIGINT         | 创建人         | FK                |
| assignee_id      | BIGINT         | 接单人         | FK, 可空          |
| status           | VARCHAR        | 状态           | INDEX             |
| version          | INT            | 乐观锁         |                   |
| title            | VARCHAR        | 标题           |                   |
| content          | TEXT           | 内容           |                   |
| deadline         | TIMESTAMP      | 截止时间       | 可空              |
| priority         | INT            | 优先级         | 可空              |
| tags             | VARCHAR[]      | 标签           | 可空              |
| attachments      | JSONB          | 附件           | 可空              |
| cancel_reason    | TEXT           | 取消原因       | 可空              |
| is_deleted       | BOOLEAN        | 软删除         | 默认 false        |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |
| accepted_at      | TIMESTAMP      | 接单时间       |                   |
| completed_at     | TIMESTAMP      | 完成时间       |                   |
| timeout_at       | TIMESTAMP      | 超时时间       |                   |
| cancelled_at     | TIMESTAMP      | 取消时间       |                   |
| created_by       | BIGINT         | 创建人         | 可空              |
| updated_by       | BIGINT         | 更新人         | 可空              |

### 3.4 order_reviews

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 评价ID         | PK, UNIQUE        |
| order_id         | BIGINT         | 订单ID         | FK, UNIQUE        |
| reviewer_id      | BIGINT         | 评价人         | FK                |
| rating           | INT            | 评分           |                   |
| comment          | TEXT           | 评价内容       |                   |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |

### 3.5 point_transactions

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 流水ID         | PK, UNIQUE        |
| user_id          | BIGINT         | 用户ID         | FK, INDEX         |
| order_id         | BIGINT         | 关联订单       | FK, 可空          |
| type             | VARCHAR        | 类型           |                   |
| amount           | INT            | 变动值         |                   |
| before_balance   | INT            | 变动前余额     |                   |
| balance          | INT            | 变动后余额     |                   |
| biz_type         | VARCHAR        | 业务类型       | 可空              |
| biz_remark       | TEXT           | 业务备注       | 可空              |
| idempotency_key  | VARCHAR        | 幂等控制       | 可空, INDEX       |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |
| ref_id           | BIGINT         | 关联业务ID     | 可空              |

### 3.6 diamond_flow

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 流水ID         | PK, UNIQUE        |
| group_id         | BIGINT         | 组ID           | FK, INDEX         |
| type             | VARCHAR        | 类型           |                   |
| amount           | INT            | 变动值         |                   |
| before_balance   | INT            | 变动前余额     |                   |
| balance          | INT            | 变动后余额     |                   |
| biz_type         | VARCHAR        | 业务类型       | 可空              |
| biz_remark       | TEXT           | 业务备注       | 可空              |
| idempotency_key  | VARCHAR        | 幂等控制       | 可空, INDEX       |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |
| ref_id           | BIGINT         | 关联业务ID     | 可空              |

### 3.7 footprints

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 足迹ID         | PK, UNIQUE        |
| group_id         | BIGINT         | 组ID           | FK, INDEX         |
| user_id          | BIGINT         | 创建人         | FK                |
| order_id         | BIGINT         | 关联订单       | FK, 可空          |
| status           | VARCHAR        | 状态           | INDEX             |
| content          | TEXT           | 内容           |                   |
| location         | VARCHAR        | 地理位置       | 可空              |
| images           | JSONB          | 图片           | 可空              |
| visibility       | VARCHAR        | 可见性         | 可空              |
| is_deleted       | BOOLEAN        | 软删除         | 默认 false        |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |
| published_at     | TIMESTAMP      | 发布时间       |                   |
| deleted_at       | TIMESTAMP      | 删除时间       | 可空              |

### 3.8 achievements

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 成就ID         | PK, UNIQUE        |
| code             | VARCHAR        | 成就编码       | UNIQUE            |
| name             | VARCHAR        | 名称           |                   |
| description      | TEXT           | 描述           |                   |
| rule_config      | JSONB          | 规则配置       |                   |
| is_active        | BOOLEAN        | 是否有效       | 默认 true         |
| extra            | JSONB          | 扩展字段       |                   |
| created_at       | TIMESTAMP      | 创建时间       |                   |

### 3.9 user_achievements

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 记录ID         | PK, UNIQUE        |
| user_id          | BIGINT         | 用户ID         | FK, INDEX         |
| achievement_id   | BIGINT         | 成就ID         | FK                |
| unlocked_at      | TIMESTAMP      | 解锁时间       |                   |
| extra            | JSONB          | 扩展字段       |                   |

### 3.10 event_log

| 字段             | 类型           | 说明           | 约束/索引         |
|------------------|----------------|----------------|-------------------|
| id               | BIGSERIAL      | 事件ID         | PK, UNIQUE        |
| event_type       | VARCHAR        | 事件类型       | INDEX             |
| payload          | JSONB          | 事件内容       |                   |
| status           | VARCHAR        | 状态           | INDEX             |
| idempotency_key  | VARCHAR        | 幂等控制       | 可空, INDEX       |
| trace_id         | VARCHAR        | 链路追踪       | 可空, INDEX       |
| error_message    | TEXT           | 失败原因       | 可空              |
| created_at       | TIMESTAMP      | 创建时间       |                   |
| processed_at     | TIMESTAMP      | 处理时间       | 可空              |
| retry_count      | INT            | 重试次数       |                   |
| extra            | JSONB          | 扩展字段       |                   |

---

## 4️⃣ 事件驱动架构

### 4.1 事件定义

- 典型事件：OrderCreatedEvent、OrderAcceptedEvent、OrderCompletedEvent、OrderReviewedEvent、FootprintPublishedEvent、WishFulfilledEvent、SignInEvent、DiamondConsumedEvent、PointChangedEvent

### 4.2 事件流转过程

1. **事件产生**：应用层/领域层操作后，写入 event_log（pending）
2. **事件消费**：Worker 轮询 event_log，按类型分发给对应 handler
3. **事件处理**：各模块监听关心的事件，处理业务（如发放 point/diamond、解锁成就、自动发布足迹等）
4. **状态更新**：处理成功则 event_log.status=done，失败则重试/记录失败

### 4.3 事件监听关系

- **order**：产生 Order* 事件
- **economy**：监听 OrderCompletedEvent、WishFulfilledEvent、SignInEvent，发放 point/diamond
- **achievement**：监听所有行为事件，异步判定成就
- **footprint**：监听 OrderCompletedEvent，自动发布足迹
- **扩容/解锁**：监听 DiamondConsumedEvent

### 4.4 幂等与不重复执行

- 事件处理需记录幂等 key（如 order_id + event_type），防止重复处理
- 事件消费用事务包裹，处理失败可重试
- event_log.status 控制消费进度

---

## 5️⃣ 关键业务流程（文本表达）

### 5.1 完成订单全链路

1. 用户 A 创建订单 → OrderCreatedEvent
2. 用户 B 接单 → OrderAcceptedEvent
3. 用户 B 完成订单 → OrderCompletedEvent
4. Worker 消费 OrderCompletedEvent：
   - 发放 point（给 B）
   - 若评价高，发放 diamond（给 group）
   - 自动发布足迹（footprint）
   - 检查成就（如连续完成、优质服务等）

### 5.2 签到流程

1. 用户签到 → SignInEvent
2. Worker 消费 SignInEvent：
   - 发放 point
   - 若连续签到，发放 diamond
   - 检查成就（如连续签到）

### 5.3 扩容流程

1. 用户申请扩容 → 校验 group diamond 足够
2. 扣除 diamond，写入 diamond_flow
3. 更新 group.footprint_capacity
4. 记录 DiamondConsumedEvent，供后续分析/成就

---

## 6️⃣ 技术实现建议（Rust/PostgreSQL）

### 6.1 Worker 实现

- 独立进程/线程定时扫描 event_log（pending），按类型分发 handler
- 消费用事务包裹，处理成功后 status=done，失败重试
- 支持多实例并发消费，event_log 可加行级锁（FOR UPDATE SKIP LOCKED）

### 6.2 事务控制

- 订单、经济、事件写入同一事务，保证一致性
- 事件消费也用事务，防止部分成功

### 6.3 Redis 用途

- 可选：用于分布式锁（如高并发接单）、幂等 key 缓存、排行榜缓存
- 不是强依赖，MVP 阶段可用 PostgreSQL 乐观锁/唯一约束

### 6.4 高并发一致性

- 订单接单/完成用乐观锁或 SQL 条件更新
- 经济变动用流水+事务，余额通过流水聚合
- 事件消费幂等，防止重复发放

---

## 7️⃣ 设计说明与落地性

- **分层解耦**：便于维护、扩展、测试
- **事件驱动**：解耦业务，提升可扩展性与异步能力
- **双货币模型**：边界清晰，避免经济系统混乱
- **流水机制**：所有变动可追溯，便于风控与审计
- **高并发安全**：乐观锁/条件更新/幂等，防止重复操作
- **配置化/可扩展**：成就、联动机制、容量等均可配置，支持未来玩法扩展

---

## 8️⃣ 项目落地与启动建议

### 8.1 项目启动建议（从0到1）

1. 明确 MVP 范围，优先实现核心业务（用户、分组、订单、经济、足迹、成就、事件、心愿、签到、通知、上传）。
2. 采用分层+模块化目录结构，领域模型优先。
3. 先建表（建议用 SQL migration 工具），表结构可根据上表直接生成。
4. 先实现事件流转主干（event_log + Worker），再串联各业务。
5. 先实现 API 层与基础认证，逐步补充业务用例。
6. 采用 Rust + PostgreSQL，必要时引入 Redis。
7. 预留扩展点（extra/JSONB、事件、配置表）。
8. 代码规范、测试、文档同步推进。

### 8.2 目录结构建议（Rust 项目）

```
src/
  api/            # API 层（REST/GraphQL/WS）
  application/    # 应用服务层
  domain/         # 领域模型（聚合根/实体/值对象/服务）
    user/
    order/
    economy/
    footprint/
    achievement/
    wish/
    sign_in/
    notification/
    event/
  infrastructure/ # 基础设施（DB/Redis/消息/外部服务）
  utils/          # 工具库
  middlewares/    # 中间件
  admin/          # 后台管理
  upload/         # 文件上传
  main.rs         # 启动入口
```

### 8.3 未来扩展建议

- 活动系统、AI推荐、第三方集成、数据分析等可独立新模块。
- 规则、配置、扩展字段、事件机制均支持热插拔。

---

本方案可直接指导开发，支持未来用户增长与新功能扩展。所有关键设计均有明确理由，兼顾落地性与可维护性，适合 Rust + PostgreSQL 技术栈实现。
