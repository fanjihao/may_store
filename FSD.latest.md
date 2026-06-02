# 心愿菜单 FSD - 生产级开发文档

版本：2026-06-02  
目标：10 万注册用户生产级产品  
技术栈：Rust + PostgreSQL，按需引入 Redis、对象存储、消息队列

---

## 1. 产品定位与范围

### 1.1 产品定位

心愿菜单是一款面向固定双人关系的组内协作产品。产品围绕“菜单/订单/心愿/积分/经验/等级/钻石/足迹”形成闭环：一方下单，一方接单完成并获得个人爱心积分，同时小组获得经验；积分可用于选择本组心愿；对方履约后由发起人打卡反馈，管理员可按反馈质量向组发放额外钻石奖励；组等级提升可扩大每日积分和经验上限。

产品核心不是公开社区，也不是泛任务平台，而是服务固定双人组内的长期互动。

### 1.2 核心边界

- 仅面向固定双人组：同一小组固定 2 人。
- 同一时刻每组只有 1 个 Buyer 和 1 个 Seller。
- 角色可互换，但历史数据绑定操作时的用户 ID，角色切换不改写历史。
- 用户可以拥有自己的长期账号，并可加入或拥有自己的组；做客好友不是一次性匿名用户。
- 无公开广场，足迹、心愿、订单、菜品均仅组内可见。
- 后端技术选型暂定 Rust + PostgreSQL 不变。
- 10 万用户目标指注册用户。
- 暂不做钻石充值；因资质原因，仅预留看广告获取钻石能力。

### 1.3 核心资产

| 资产 | 归属 | 用途 | 限制 |
| --- | --- | --- | --- |
| 爱心积分 `love_point` | 用户 + 小组 | 本组心愿选择 | 跨组不可使用，不可提现，不可转账 |
| 组钻石 `diamond` | 小组 | 扩容、解锁能力、运营奖励 | 暂不支持充值，预留广告获取 |
| 组经验 `group_exp` | 小组 | 组等级升级，扩大每日积分/经验上限 | 仅通过订单、活动等后台配置来源获取 |

### 1.4 阶段目标

| 阶段 | 目标 | 必须交付 | 暂缓交付 |
| --- | --- | --- | --- |
| MVP | 支撑真实双人组使用 | 微信登录、双人组、角色、菜品、订单、爱心积分、心愿、签到、通知、上传 | AI 推荐、商业化、公开内容 |
| Beta | 支撑 1 万注册用户 | 管理后台、审核、监控告警、灰度配置、数据看板、压测 | 多区域部署、复杂活动 |
| V1.0 | 支撑 10 万注册用户 | 高可用部署、备份恢复、风控、审计、隐私合规、上线验收 | 付费充值、公开广场 |

---

## 2. 用户、组与角色模型

### 2.1 用户账号

用户必须是长期账号。微信小程序登录以微信 `openid` 绑定用户，后续可扩展手机号登录。客人也使用自己的账号，并可拥有自己的小组。

### 2.2 双人组

- 每个 `association_group` 固定 2 名正式成员。
- 组内数据与其他组严格隔离。
- 组内保存当前角色映射：`buyer_user_id`、`seller_user_id`。
- 用户跨多个组时，爱心积分按 `user_id + group_id` 独立计算。

### 2.3 角色权限

| 角色 | 权限范围 | 积分获取规则 |
| --- | --- | --- |
| Buyer 下单人 | 创建本组订单；参与心愿协商；浏览心愿池；有积分时可选择心愿 | 订单无任何积分入账；想获得积分必须切换为 Seller |
| Seller 接单人 | 接单并完成本组订单或做客订单；参与心愿协商；消耗积分选择心愿 | 完成本组订单或主人家做客订单后，系统按后台规则自动发放爱心积分，并为小组增加经验 |

### 2.4 角色互换

默认前置条件：

- 小组无未完结在途订单。
- 操作人无 `CLAIMED` 状态且自己作为发起人或履约人的在途心愿。
- 互换后当前 Buyer 与 Seller 对调。
- 历史订单、心愿、积分流水均保留操作时用户 ID 和当时角色快照。

配置开关：

| 配置项 | 默认值 | 说明 |
| --- | --- | --- |
| `swap_ignore_ongoing_wish` | `false` | 为 `true` 时，允许带在途心愿互换身份；已选择心愿的发起人和履约人不变 |

### 2.5 做客关系

- 做客用户必须使用长期账号。
- 做客用户通过邀请链接直接访问主人家厨房。
- 做客用户可查看主人家厨房菜单、选择菜品、下单、标记订单偏好或备注。
- 做客订单归属主人家小组，由主人家 Seller 完成。
- 做客订单与做客用户自己的组数据隔离，但订单列表需要同时展示“自己组订单”和“我发起的做客订单”。
- 做客用户仅可查看邀请授权范围内的主人家菜单、自己创建的做客订单以及必要的订单状态。
- 做客订单完成后，主人家 Seller 可获得爱心积分；做客用户不获得主人组爱心积分。
- 做客订单积分必须接入风控，避免通过邀请链接刷单刷积分。

---

## 3. 总体架构

### 3.1 分层架构

- API 层：RESTful API、WebSocket、参数校验、认证授权、错误码。
- 应用层：业务用例编排、事务边界、幂等、领域事件发布。
- 领域层：订单状态机、心愿状态机、经济规则、角色互换、成就规则。
- 事件层：事件定义、事件日志、Worker 消费、重试、死信。
- 基础设施层：PostgreSQL、Redis、对象存储、微信接口、广告接口、内容审核。

### 3.2 模块划分

| 模块 | 职责 |
| --- | --- |
| `user` | 用户、登录、账号状态、黑名单 |
| `group` | 双人组、成员、角色映射、邀请 |
| `food` | 菜品、标签、食材、图片、步骤 |
| `order` | 订单、状态机、评价、催办、附件 |
| `wish` | 心愿创建、协商、兑换、冻结积分、打卡、审核 |
| `economy` | 爱心积分、组钻石、流水、对账 |
| `sign_in` | 签到、连续签到、组钻石奖励 |
| `footprint` | 组内足迹、纪念内容、图片 |
| `achievement` | 成就规则、成就墙、事件判定 |
| `notification` | 系统通知、订单通知、心愿通知、未读数 |
| `upload` | 文件上传、对象存储、内容审核 |
| `admin` | 后台配置、审核、用户管理、数据统计 |
| `event` | 事件日志、Worker、幂等处理 |
| `ws` | WebSocket 实时通知 |
| `dashboard` | 组内看板、运营看板、指标聚合 |

### 3.3 核心原则

- 所有经济变动必须写流水，禁止直接改余额。
- 订单状态、心愿状态、积分冻结/扣减必须具备状态机约束。
- 写接口必须支持幂等。
- 所有组内资源必须校验 `group_id` 和成员关系。
- 异步联动通过事件驱动，避免模块间强耦合。
- 所有关键操作保留审计字段和 trace_id。

---

## 4. 登录、邀请与权限

### 4.1 微信登录流程

1. 前端调用 `uni.login()` 或微信小程序登录接口获取 `code`。
2. 前端请求 `POST /api/auth/wechat-login`。
3. 后端使用 `code` 调用微信接口换取 `openid` 和 `session_key`。
4. 后端按 `openid` 查找用户。
5. 已存在用户返回访问令牌、刷新令牌和用户信息。
6. 不存在用户创建账号或进入补充资料流程。

安全要求：

- 前端不得直接传入可信 `openid` 创建正式用户。
- 日志中禁止记录 `code`、`session_key`、JWT、手机号明文。
- JWT 使用短有效期访问令牌 + 可撤销刷新令牌。

### 4.2 组邀请

- 组内成员可以生成邀请链接或邀请码。
- 受邀用户登录后加入组。
- 若组已满 2 人，邀请失效。
- 邀请码必须设置有效期、使用次数和撤销能力。

### 4.3 权限校验

所有资源接口必须校验：

- 用户是否登录且状态正常。
- 用户是否属于当前组。
- 当前组是否为资源所属组。
- 当前用户角色是否有权执行操作。
- 做客用户是否拥有该邀请授权范围。

---

## 5. 菜品系统

### 5.1 业务规则

- 菜品属于小组，不跨组共享。
- 默认由 Seller 维护，可通过组配置允许 Buyer 参与维护。
- 菜品可用于 Buyer 创建订单，也可用于做客好友点菜。

### 5.2 菜品字段

| 字段 | 说明 |
| --- | --- |
| `id` | 菜品 ID |
| `group_id` | 所属小组 |
| `name` | 菜品名称 |
| `description` | 描述 |
| `images` | 图片列表 |
| `tags` | 标签 |
| `ingredients` | 食材 |
| `steps` | 步骤 |
| `status` | `ACTIVE`、`HIDDEN`、`DELETED` |
| `created_by` | 创建人 |
| `updated_by` | 更新人 |

---

## 6. 订单系统

### 6.1 订单类型

| 类型 | 说明 |
| --- | --- |
| `NORMAL` | 组内普通订单 |
| `GUEST` | 做客订单，由受邀好友通过邀请链接访问主人家厨房后创建，主人组 Seller 完成 |

### 6.2 订单状态机

```text
CREATED
  -> ACCEPTED
  -> PRODUCTION_COMPLETED
  -> CONFIRMED_COMPLETED
  -> CONFIRMED_INCOMPLETE

CREATED -> REJECTED
CREATED -> CANCELLED
CREATED/ACCEPTED -> TIMEOUT
```

### 6.3 积分发放规则

- 仅 Seller 完成订单后获得爱心积分。
- Buyer 创建或确认本组订单不获得爱心积分。
- 做客订单由主人家 Seller 完成后，给主人家 Seller 发放爱心积分。
- 做客用户不获得主人组爱心积分，也不能消耗主人组爱心积分。
- 订单积分规则全部由管理员在后台配置，用户侧不可配置订单积分。
- 可按订单类型配置不同积分：`NORMAL` 和 `GUEST` 可分别配置默认值、上限、每日计分次数。
- 每个用户每天在每个组内有爱心积分获取上限；超过上限后，订单仍可完成，但不再增加爱心积分。
- 每个组每天有经验获取上限；超过上限后，订单仍可完成，但不再增加组经验。
- 组等级提升后可扩大每日爱心积分上限和每日经验上限，具体等级经验和上限由管理员后台配置。
- 订单确认完成后，未超上限的爱心积分写入 `love_point_transactions`。
- 订单确认完成后，未超上限的组经验写入 `group_exp_transactions`。
- 重复确认、重复事件消费不得重复发放爱心积分和组经验。

### 6.4 做客订单列表与标记

- 用户订单列表需要聚合展示两类订单：自己所属组订单、自己发起的做客订单。
- 做客订单需标记主人家信息、订单类型 `GUEST`、当前状态和完成方。
- 做客用户可对自己发起的做客订单添加备注或标记，例如口味偏好、忌口、到访时间。
- 主人家组内列表需要展示做客订单，并区分普通订单与做客订单。
- 做客订单只允许创建人、主人家组内 2 人和管理员查看。

### 6.5 订单防刷积分与经验

- 所有订单发放爱心积分和组经验前必须通过风控校验，不仅限于做客订单。
- 防刷的第一层规则是每日上限：用户每日爱心积分上限、组每日经验上限、订单类型每日计分次数上限。
- 超出每日上限的订单仍可正常完成、评价、生成足迹，但 `point_grant_status=REJECTED_LIMIT`，不增加爱心积分；`exp_grant_status=REJECTED_LIMIT`，不增加组经验。
- 同一做客用户对同一主人家小组每日可计分做客订单数量默认不超过 1 单。
- 同一邀请链接每日可计分订单数量、总计分订单数量需要后台可配置。
- 同一设备、同一 IP、同一微信账号、同一主人家小组短时间内大量创建做客订单时进入风险状态。
- 风险订单可正常流转，但积分和经验发放进入 `PENDING_REVIEW` 或不计分，等待管理员审核。
- 被判定为刷分的订单不发放爱心积分和组经验；已发放的需要通过补偿流水扣回。
- 风控结果、命中规则、处理人和处理时间必须写入审计日志。

### 6.6 并发安全

- 接单使用条件更新：仅 `CREATED` 状态可接单。
- 完成、确认、取消、超时均必须校验当前状态。
- 关键写接口使用 `Idempotency-Key`。
- 订单积分发放以 `order_id + event_type` 作为幂等键。

---

## 7. 心愿系统

### 7.1 业务目标

心愿系统用于把“双人之间想要对方完成的事情”沉淀为组内心愿池。用户 A 创建心愿，双方协商爱心积分价格和履约期限，双方线上确认后进入心愿池。A 通过完成 B 的菜品订单获取爱心积分，攒够后选择该心愿并冻结积分；随后 B 必须在约定期限内线下履约；A 完成打卡反馈后，心愿完成，积分正式扣除。管理员只对打卡质量进行后台查看，并可发放额外组钻石奖励。

### 7.2 数据隔离与资产规则

- 心愿归属小组，仅组内 2 人可查看和操作。
- 爱心积分绑定 `user_id + group_id`，跨组不可使用。
- 爱心积分仅用于本组心愿选择，不可提现、不可转账。
- 心愿选择使用冻结机制，打卡反馈完成后才正式扣减。
- 履约人与自然人用户 ID 绑定，与当前 Buyer/Seller 角色无关。
- 角色互换不改变已选择心愿的履约人、选择人和历史责任。

### 7.3 心愿角色与自然人职责

心愿流程以自然人用户为核心，不以当前角色为最终责任主体。创建心愿时的发起人记为 `requester_id`，对方自然人记为 `fulfiller_id`。

| 操作 | 发起人 requester | 履约人 fulfiller | 管理员 |
| --- | --- | --- | --- |
| 创建心愿 | 是 | 否 | 否 |
| 协商积分价格 | 是 | 是 | 否 |
| 协商履约期限 | 是 | 是 | 否 |
| 线上确认 | 是 | 是 | 否 |
| 选择心愿并冻结积分 | 是 | 否 | 否 |
| 线下履约 | 否 | 是 | 否 |
| 打卡反馈 | 是 | 否 | 否 |
| 查看履约质量并发额外钻石 | 否 | 否 | 是 |

### 7.4 整体业务流程

```text
A 创建心愿
  -> DRAFT
  -> 双方协商积分价格 + 履约期限
  -> 双方线上确认
  -> CREATED 进入组内心愿池

A 通过完成 B 的菜品订单获取爱心积分
  -> 受每日积分上限限制
  -> 完成订单同时获取组经验
  -> 受每日经验上限限制
  -> 组等级提升可扩大每日积分/经验上限

A 攒够积分
  -> 选择该心愿
  -> 冻结 A 的爱心积分
  -> CLAIMED，绑定履约人 B，生成履约截止时间

B 在期限内线下履约
  -> A 提交打卡反馈
  -> FINISHED，正式扣除 A 的冻结积分
  -> 管理员后台查看质量，可额外发放组钻石

B 逾期未履约
  -> EXPIRED
  -> 退还 A 的冻结积分
  -> 记录 B 的逾期履约记录
  -> 心愿按配置回池或关闭

任意非终态
  -> 双方协商一致关闭 -> CLOSED
```

### 7.5 状态定义

| 状态 | 说明 | 可操作 |
| --- | --- | --- |
| `DRAFT` | 发起人创建草稿 | 进入协商、关闭 |
| `NEGOTIATING` | 双方协商积分和履约期限 | 报价、还价、修改期限、确认、关闭 |
| `CREATED` | 双方已确认，进入组内心愿池 | 选择心愿、关闭 |
| `CLAIMED` | 发起人已选择，积分已冻结，待履约 | 打卡反馈、逾期处理、关闭 |
| `FINISHED` | 已履约并打卡，积分正式扣减 | 终态，管理员可追加钻石奖励 |
| `EXPIRED` | 履约人逾期未履约，积分已退还 | 终态或按配置回到 `CREATED` |
| `CLOSED` | 双方关闭或作废 | 终态 |

### 7.6 关键业务规则

- 任一组内成员都可以创建心愿；创建后发起人为 `requester_id`，另一名组成员为默认 `fulfiller_id`。
- 双方必须线上确认积分价格和履约期限，心愿才可进入 `CREATED`。
- `CREATED` 状态才可被发起人选择。
- 选择心愿时检查发起人爱心积分余额，余额不足拒绝。
- 选择心愿时冻结发起人的爱心积分，不立即扣减。
- 同一心愿同一时间只能有一次有效选择。
- `CLAIMED` 生成履约截止时间，截止时间来自双方协商的履约期限。
- 履约人必须在期限内线下履约；系统以发起人打卡反馈作为履约完成证明。
- 发起人提交打卡反馈后，冻结积分转正式扣减，心愿进入 `FINISHED`。
- 管理员不负责判定心愿是否完成，只查看反馈质量并决定是否额外发放组钻石。
- 履约逾期时退还冻结积分，记录履约人的逾期履约记录。
- 每个用户需要展示其作为履约人时的履约率、逾期次数、平均履约时长等信息，组内双方可见。
- 用户退出组前必须结清：不能存在自己作为发起人或履约人的未完结心愿、冻结积分、待履约记录。

### 7.7 积分冻结与扣减

| 阶段 | 积分动作 |
| --- | --- |
| 创建/协商/入池 | 无积分变动 |
| 选择心愿 | 生成冻结流水，增加 `frozen_love_point`，减少可用余额 |
| 履约中 | 无积分变动 |
| 打卡反馈完成 | 冻结转正式扣减，生成扣减流水 |
| 履约逾期 | 解冻退回，生成解冻流水 |
| 关闭 | 如存在冻结积分，必须解冻 |

### 7.8 履约率与退出结清

履约统计按自然人和小组维度计算，和当前角色无关。

| 指标 | 说明 |
| --- | --- |
| `fulfillment_total` | 作为履约人的总心愿数 |
| `fulfillment_finished` | 按期完成数量 |
| `fulfillment_expired` | 逾期数量 |
| `fulfillment_rate` | 按期完成率 |
| `avg_fulfillment_hours` | 平均履约时长 |
| `pending_fulfillment_count` | 当前待履约数量 |

退出组前必须满足：

- 无自己发起且未完结的心愿。
- 无自己作为履约人且未完结的心愿。
- 无本组冻结爱心积分。
- 无待处理的逾期补偿或管理员钻石奖励。

### 7.9 管理员质量奖励

管理员查看打卡反馈后，可根据质量发放额外组钻石奖励。

| 字段 | 说明 |
| --- | --- |
| `quality_reviewer_id` | 查看/奖励管理员 |
| `quality_level` | `NONE`、`NORMAL`、`GOOD`、`EXCELLENT` |
| `quality_remark` | 质量备注 |
| `diamond_reward` | 本次额外发放钻石 |
| `quality_reviewed_at` | 查看时间 |

要求：

- 钻石奖励必须走 `diamond_transactions`。
- 同一心愿额外钻石奖励必须幂等。
- 可配置质量等级对应钻石、单次钻石上限和管理员单日发放上限。
- 未发钻石时也可记录已查看质量，便于运营统计。

### 7.10 心愿事件

| 事件 | 触发时机 |
| --- | --- |
| `WishCreatedEvent` | 用户创建心愿 |
| `WishNegotiatingEvent` | 进入协商 |
| `WishAgreementConfirmedEvent` | 双方确认积分和期限 |
| `WishSelectedEvent` | 发起人选择心愿并冻结积分 |
| `WishFeedbackSubmittedEvent` | 发起人提交打卡反馈 |
| `WishFinishedEvent` | 心愿完成并扣减冻结积分 |
| `WishExpiredEvent` | 履约逾期并退还积分 |
| `WishQualityRewardedEvent` | 管理员发放额外组钻石 |
| `WishClosedEvent` | 心愿关闭 |

---

## 8. 经济系统

### 8.1 爱心积分 love_point

- 归属：用户 + 小组。
- 来源：Seller 完成本组订单或主人家做客订单。
- 用途：兑换本组心愿。
- 限制：不可提现、不可转账、不可跨组使用。
- 余额模型：可用余额 + 冻结余额。
- 获取限制：按用户 + 小组 + 自然日计算每日获取上限，超出后订单完成不再增加爱心积分。

### 8.2 组钻石 diamond

- 归属：小组。
- 来源：签到奖励、心愿质量奖励、运营补偿、预留广告奖励。
- 用途：足迹容量扩展、菜品/标签容量扩展、玩法解锁。
- 暂不支持充值。

### 8.3 组经验 group_exp

- 归属：小组。
- 来源：完成本组普通订单、主人家做客订单、活动等后台配置来源。
- 用途：提升组等级。
- 获取限制：按小组 + 自然日计算每日经验上限，超出后订单完成不再增加组经验。
- 等级作用：组等级提升后，可扩大每日爱心积分上限、每日经验上限、容量上限或其他玩法能力。
- 等级经验表、每日上限、订单经验值全部由管理员后台配置。

### 8.4 流水规则

所有变动必须写流水：

- `love_point_transactions`
- `diamond_transactions`
- `group_exp_transactions`

流水必须包含：

- `user_id` 或 `group_id`
- `amount`
- `balance_before`
- `balance_after`
- `frozen_before`
- `frozen_after`
- `biz_type`
- `biz_id`
- `idempotency_key`
- `trace_id`
- `created_at`

### 8.5 对账

- 每日任务聚合流水，与余额快照比对。
- 发现不一致时告警并生成修复任务。
- 经济修复必须走补偿流水，不允许后台直接改余额。

---

## 9. 签到、足迹与成就

### 9.1 签到

- 签到奖励发放组钻石。
- 同一天同一用户同一组只能签到一次。
- 双方当日均签到时可触发额外组钻石奖励。
- 签到按用户本地时区计算连续天数，但数据库统一 UTC 存储。

### 9.2 足迹

- 足迹仅组内可见，无公开广场。
- 订单完成、心愿完成后可生成纪念足迹。
- 用户可上传文字、图片和可选位置。
- 位置默认可选，用户可选择不展示位置。
- 图片需通过内容安全审核。

### 9.3 成就

- 成就由事件驱动异步判定。
- 支持用户成就和组成就。
- 成就规则通过配置表管理。
- 成就解锁必须幂等。

---

## 10. 通知与 WebSocket

### 10.1 通知类型

| 类型 | 场景 |
| --- | --- |
| 订单通知 | 创建、接单、完成、确认、超时 |
| 心愿通知 | 协商、入池、兑换、打卡、审核 |
| 积分通知 | 爱心积分入账、冻结、扣减、解冻 |
| 钻石通知 | 组钻石发放、消耗 |
| 系统通知 | 邀请、角色互换、管理员公告 |

### 10.2 WebSocket 消息格式

```json
{
  "type": "notification",
  "data": {},
  "trace_id": "string",
  "sent_at": "2026-06-02T00:00:00Z"
}
```

### 10.3 连接要求

- WebSocket 服务独立部署。
- 连接后 10 秒内必须完成鉴权。
- 心跳间隔 30 秒。
- 多实例场景使用 Redis 维护用户在线状态或连接路由。
- 离线消息写入通知表，用户重新上线后拉取。

---

## 11. 数据库设计

### 11.1 通用字段

关键业务表建议包含：

- `id`
- `created_at`
- `updated_at`
- `created_by`
- `updated_by`
- `is_deleted`
- `extra JSONB`
- `trace_id`

### 11.2 users

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 用户 ID |
| `openid` | VARCHAR | 微信 openid，唯一 |
| `nickname` | VARCHAR | 昵称 |
| `avatar_url` | VARCHAR | 头像 |
| `phone` | VARCHAR | 手机号，可空 |
| `status` | VARCHAR | `ACTIVE`、`BANNED`、`DELETED` |
| `last_login_at` | TIMESTAMP | 最后登录时间 |
| `created_at` | TIMESTAMP | 创建时间 |

### 11.3 association_groups

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 小组 ID |
| `name` | VARCHAR | 组名 |
| `buyer_user_id` | BIGINT | 当前 Buyer |
| `seller_user_id` | BIGINT | 当前 Seller |
| `diamond_balance` | BIGINT | 组钻石余额 |
| `level` | INT | 组等级 |
| `exp` | BIGINT | 当前等级内经验或累计经验 |
| `footprint_capacity` | INT | 足迹容量 |
| `settings` | JSONB | 组配置 |
| `status` | VARCHAR | 状态 |

### 11.4 group_members

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 记录 ID |
| `group_id` | BIGINT | 小组 ID |
| `user_id` | BIGINT | 用户 ID |
| `member_status` | VARCHAR | `ACTIVE`、`LEFT` |
| `joined_at` | TIMESTAMP | 加入时间 |

约束：`UNIQUE(group_id, user_id)`；同一组 `ACTIVE` 成员数量不得超过 2。

### 11.5 user_group_points

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 记录 ID |
| `user_id` | BIGINT | 用户 ID |
| `group_id` | BIGINT | 小组 ID |
| `available_love_point` | BIGINT | 可用爱心积分 |
| `frozen_love_point` | BIGINT | 冻结爱心积分 |
| `updated_at` | TIMESTAMP | 更新时间 |

约束：`UNIQUE(user_id, group_id)`。

### 11.6 orders

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 订单 ID |
| `group_id` | BIGINT | 所属小组 |
| `type` | VARCHAR | `NORMAL`、`GUEST` |
| `creator_id` | BIGINT | 创建人 |
| `assignee_id` | BIGINT | 接单人 |
| `guest_user_id` | BIGINT | 做客用户，仅 `GUEST` 订单有值 |
| `guest_invite_id` | BIGINT | 做客邀请 ID，仅 `GUEST` 订单有值 |
| `guest_remark` | TEXT | 做客备注、口味偏好、忌口、到访时间等 |
| `guest_mark_tags` | JSONB | 做客订单标记 |
| `creator_role_snapshot` | VARCHAR | 创建时角色 |
| `assignee_role_snapshot` | VARCHAR | 接单时角色 |
| `status` | VARCHAR | 订单状态 |
| `love_point_reward` | INT | 完成奖励积分 |
| `point_grant_status` | VARCHAR | `NONE`、`PENDING_REVIEW`、`GRANTED`、`REJECTED`、`REVOKED`、`REJECTED_LIMIT` |
| `group_exp_reward` | INT | 完成奖励组经验 |
| `exp_grant_status` | VARCHAR | `NONE`、`PENDING_REVIEW`、`GRANTED`、`REJECTED`、`REVOKED`、`REJECTED_LIMIT` |
| `risk_status` | VARCHAR | `PASS`、`SUSPECT`、`BLOCKED` |
| `risk_detail` | JSONB | 命中风控规则详情 |
| `title` | VARCHAR | 标题 |
| `content` | TEXT | 内容 |
| `deadline` | TIMESTAMP | 截止时间 |
| `version` | INT | 乐观锁 |
| `created_at` | TIMESTAMP | 创建时间 |
| `accepted_at` | TIMESTAMP | 接单时间 |
| `completed_at` | TIMESTAMP | 完成时间 |
| `confirmed_at` | TIMESTAMP | 确认时间 |

### 11.7 wishes

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 心愿 ID |
| `group_id` | BIGINT | 所属小组 |
| `created_by` | BIGINT | 创建人 |
| `requester_id` | BIGINT | 发起人，即选择心愿和支付积分的人 |
| `fulfiller_id` | BIGINT | 履约人，即线下完成心愿的人 |
| `creator_role_snapshot` | VARCHAR | 创建时角色快照 |
| `name` | VARCHAR | 心愿名称 |
| `description` | TEXT | 描述 |
| `initial_cost` | INT | 初始报价 |
| `final_cost` | INT | 双方确认后的爱心积分价格 |
| `fulfillment_deadline_hours` | INT | 双方确认的履约期限小时数 |
| `status` | VARCHAR | 心愿状态 |
| `selected_by` | BIGINT | 选择心愿的人，通常等于 `requester_id` |
| `selected_at` | TIMESTAMP | 选择时间 |
| `fulfillment_due_at` | TIMESTAMP | 履约截止时间 |
| `fulfilled_at` | TIMESTAMP | 发起人打卡确认履约时间 |
| `expired_at` | TIMESTAMP | 逾期时间 |
| `quality_review_status` | VARCHAR | 质量查看状态 |
| `quality_reviewer_id` | BIGINT | 质量查看管理员 |
| `quality_remark` | TEXT | 质量备注 |
| `diamond_reward` | INT | 质量奖励发放钻石 |
| `finished_at` | TIMESTAMP | 完成时间 |
| `closed_at` | TIMESTAMP | 关闭时间 |
| `version` | INT | 乐观锁 |

### 11.8 wish_negotiations

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 协商记录 ID |
| `wish_id` | BIGINT | 心愿 ID |
| `group_id` | BIGINT | 小组 ID |
| `operator_id` | BIGINT | 操作人 |
| `operator_role_snapshot` | VARCHAR | 操作时角色 |
| `action` | VARCHAR | `QUOTE`、`COUNTER`、`SET_DEADLINE`、`ACCEPT`、`REJECT`、`CLOSE` |
| `cost` | INT | 本次报价 |
| `deadline_hours` | INT | 本次协商履约期限 |
| `remark` | TEXT | 备注 |
| `created_at` | TIMESTAMP | 创建时间 |

### 11.9 wish_checkins

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 打卡 ID |
| `wish_id` | BIGINT | 心愿 ID |
| `user_id` | BIGINT | 提交人，必须为发起人 requester |
| `content` | TEXT | 文字内容 |
| `location` | VARCHAR | 可选位置 |
| `images` | JSONB | 图片列表 |
| `created_at` | TIMESTAMP | 提交时间 |

### 11.10 love_point_transactions

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 流水 ID |
| `user_id` | BIGINT | 用户 ID |
| `group_id` | BIGINT | 小组 ID |
| `type` | VARCHAR | `EARN`、`FREEZE`、`UNFREEZE`、`DEDUCT`、`ADJUST` |
| `amount` | BIGINT | 变动数量 |
| `available_before` | BIGINT | 可用变动前 |
| `available_after` | BIGINT | 可用变动后 |
| `frozen_before` | BIGINT | 冻结变动前 |
| `frozen_after` | BIGINT | 冻结变动后 |
| `biz_type` | VARCHAR | 业务类型 |
| `biz_id` | BIGINT | 业务 ID |
| `idempotency_key` | VARCHAR | 幂等键 |
| `trace_id` | VARCHAR | 链路 ID |
| `created_at` | TIMESTAMP | 创建时间 |

### 11.11 diamond_transactions

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 流水 ID |
| `group_id` | BIGINT | 小组 ID |
| `type` | VARCHAR | `EARN`、`CONSUME`、`ADJUST` |
| `amount` | BIGINT | 变动数量 |
| `balance_before` | BIGINT | 变动前 |
| `balance_after` | BIGINT | 变动后 |
| `biz_type` | VARCHAR | 业务类型 |
| `biz_id` | BIGINT | 业务 ID |
| `idempotency_key` | VARCHAR | 幂等键 |
| `trace_id` | VARCHAR | 链路 ID |
| `created_at` | TIMESTAMP | 创建时间 |

### 11.12 group_exp_transactions

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 流水 ID |
| `group_id` | BIGINT | 小组 ID |
| `type` | VARCHAR | `EARN`、`ADJUST`、`REVOKE` |
| `amount` | BIGINT | 变动数量 |
| `exp_before` | BIGINT | 变动前经验 |
| `exp_after` | BIGINT | 变动后经验 |
| `level_before` | INT | 变动前等级 |
| `level_after` | INT | 变动后等级 |
| `biz_type` | VARCHAR | 业务类型 |
| `biz_id` | BIGINT | 业务 ID |
| `idempotency_key` | VARCHAR | 幂等键 |
| `trace_id` | VARCHAR | 链路 ID |
| `created_at` | TIMESTAMP | 创建时间 |

### 11.13 daily_reward_counters

用于订单奖励每日上限控制。发放爱心积分或组经验前，必须在事务内按自然日更新该表；超过上限则订单完成但不写奖励流水。

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 记录 ID |
| `stat_date` | DATE | 统计日期，按组配置时区折算 |
| `group_id` | BIGINT | 小组 ID |
| `user_id` | BIGINT | 用户 ID；组经验统计可为空 |
| `love_point_earned` | BIGINT | 当日已获得爱心积分 |
| `group_exp_earned` | BIGINT | 当日小组已获得经验 |
| `normal_order_count` | INT | 当日计奖普通订单数 |
| `guest_order_count` | INT | 当日计奖做客订单数 |
| `created_at` | TIMESTAMP | 创建时间 |
| `updated_at` | TIMESTAMP | 更新时间 |

唯一约束：

- 用户积分计数：`UNIQUE(stat_date, group_id, user_id)`。
- 组经验计数：可使用 `user_id = 0` 或独立唯一约束 `UNIQUE(stat_date, group_id)`。

### 11.14 event_log

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGSERIAL | 事件 ID |
| `event_type` | VARCHAR | 事件类型 |
| `payload` | JSONB | 事件内容 |
| `status` | VARCHAR | `PENDING`、`PROCESSING`、`DONE`、`FAILED`、`DEAD` |
| `retry_count` | INT | 重试次数 |
| `idempotency_key` | VARCHAR | 幂等键 |
| `trace_id` | VARCHAR | 链路 ID |
| `error_message` | TEXT | 错误信息 |
| `created_at` | TIMESTAMP | 创建时间 |
| `processed_at` | TIMESTAMP | 处理时间 |

### 11.13 关键索引

| 表 | 索引 |
| --- | --- |
| `users` | `uniq_users_openid`、`idx_users_status` |
| `association_groups` | `idx_groups_buyer`、`idx_groups_seller` |
| `group_members` | `uniq_group_user`、`idx_group_members_user` |
| `orders` | `idx_orders_group_status_created_at`、`idx_orders_assignee_status` |
| `wishes` | `idx_wishes_group_status`、`idx_wishes_requester_status`、`idx_wishes_fulfiller_status` |
| `love_point_transactions` | `uniq_love_point_idempotency_key`、`idx_love_point_user_group_created_at` |
| `diamond_transactions` | `uniq_diamond_idempotency_key`、`idx_diamond_group_created_at` |
| `group_exp_transactions` | `uniq_group_exp_idempotency_key`、`idx_group_exp_group_created_at` |
| `daily_reward_counters` | `uniq_daily_reward_user`、`idx_daily_reward_group_date` |
| `event_log` | `idx_event_status_created_at`、`uniq_event_idempotency_key`、`idx_event_trace_id` |

---

## 12. 事件驱动架构

### 12.1 事件写入

- 应用层完成核心业务事务时，同事务写入 `event_log`。
- Worker 异步消费 `PENDING` 事件。
- 多 Worker 使用 `FOR UPDATE SKIP LOCKED` 或队列竞争消费。
- 事件处理成功标记 `DONE`，失败进入重试。
- 超过最大重试次数进入 `DEAD`，等待人工或补偿任务处理。

### 12.2 典型事件关系

| 来源模块 | 事件 | 消费模块 |
| --- | --- | --- |
| order | `OrderConfirmedCompletedEvent` | economy、achievement、footprint、notification |
| wish | `WishSelectedEvent` | economy、notification |
| wish | `WishFinishedEvent` | economy、achievement、footprint、notification |
| wish | `WishExpiredEvent` | economy、notification |
| wish | `WishQualityRewardedEvent` | economy、notification |
| sign_in | `SignInEvent` | economy、achievement、notification |
| group | `RoleSwappedEvent` | notification、achievement |

### 12.3 幂等要求

- 每个事件必须携带业务幂等键。
- 经济发放类事件必须通过流水唯一键防重。
- 通知类事件允许至少一次投递，但通知记录必须防重复。

---

## 13. API 设计

### 13.1 通用约定

- 返回体统一包含 `code`、`message`、`data`、`trace_id`。
- 时间统一 UTC 存储，接口返回 ISO 8601。
- 写接口要求 `Idempotency-Key`。
- 列表优先使用 cursor 分页。
- 后台接口与用户接口权限隔离。

### 13.2 核心 API 示例

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/api/auth/wechat-login` | 微信登录 |
| `POST` | `/api/groups` | 创建双人组 |
| `POST` | `/api/groups/{group_id}/invite` | 创建邀请 |
| `POST` | `/api/groups/{group_id}/swap-role` | 角色互换 |
| `GET` | `/api/groups/{group_id}/settlement-check` | 退出组前结清检查 |
| `GET` | `/api/groups/{group_id}/foods` | 菜品列表 |
| `GET` | `/api/kitchens/invitations/{invite_code}` | 通过邀请链接访问主人家厨房 |
| `GET` | `/api/kitchens/invitations/{invite_code}/foods` | 查看主人家厨房菜单 |
| `POST` | `/api/groups/{group_id}/orders` | 创建订单 |
| `POST` | `/api/kitchens/invitations/{invite_code}/orders` | 创建做客订单 |
| `GET` | `/api/orders` | 聚合查询自己组订单和我发起的做客订单 |
| `POST` | `/api/orders/{order_id}/accept` | 接单 |
| `POST` | `/api/orders/{order_id}/complete` | Seller 完成订单 |
| `POST` | `/api/orders/{order_id}/confirm` | Buyer 确认订单 |
| `POST` | `/api/groups/{group_id}/wishes` | 创建心愿 |
| `POST` | `/api/wishes/{wish_id}/quote` | 协商报价 |
| `POST` | `/api/wishes/{wish_id}/deadline` | 协商履约期限 |
| `POST` | `/api/wishes/{wish_id}/confirm-agreement` | 双方线上确认积分和期限 |
| `POST` | `/api/wishes/{wish_id}/reject` | 拒绝或关闭协商 |
| `POST` | `/api/wishes/{wish_id}/select` | 选择心愿并冻结积分 |
| `POST` | `/api/wishes/{wish_id}/feedback` | 发起人提交打卡反馈 |
| `GET` | `/api/groups/{group_id}/fulfillment-stats` | 查看组内双方履约统计 |
| `POST` | `/api/admin/wishes/{wish_id}/quality-reward` | 管理员按反馈质量发放额外组钻石 |
| `POST` | `/api/admin/orders/{order_id}/reward-review` | 管理员审核风险订单积分和经验发放 |

### 13.3 核心错误码

| 错误码 | 说明 |
| --- | --- |
| `AUTH_INVALID_TOKEN` | 登录态无效 |
| `GROUP_MEMBER_LIMIT_EXCEEDED` | 小组成员超过 2 人 |
| `PERMISSION_DENIED` | 无权限 |
| `ROLE_NOT_ALLOWED` | 当前角色不允许操作 |
| `ROLE_SWAP_BLOCKED_BY_ORDER` | 存在未完结订单，禁止互换 |
| `ROLE_SWAP_BLOCKED_BY_WISH` | 操作人存在在途心愿，禁止互换 |
| `ORDER_STATUS_INVALID` | 订单状态不允许当前操作 |
| `WISH_STATUS_INVALID` | 心愿状态不允许当前操作 |
| `LOVE_POINT_INSUFFICIENT` | 爱心积分不足 |
| `DAILY_REWARD_LIMIT_REACHED` | 每日积分或经验上限已达，订单完成但不再发放奖励 |
| `GROUP_EXIT_SETTLEMENT_REQUIRED` | 退出组前仍有未结清订单、心愿、冻结积分或履约责任 |
| `IDEMPOTENCY_CONFLICT` | 幂等键对应请求内容冲突 |
| `UPLOAD_CONTENT_REJECTED` | 上传内容审核未通过 |

---

## 14. 生产容量与非功能要求

### 14.1 容量假设

| 项目 | 目标 |
| --- | --- |
| 注册用户 | 100,000 |
| 日活用户 | 15,000 - 30,000 |
| 峰值在线用户 | 3,000 - 8,000 |
| 峰值 HTTP QPS | 300 - 800 |
| 峰值 WebSocket 连接 | 5,000 |
| 日订单量 | 20,000 - 80,000 |
| 日事件量 | 100,000 - 500,000 |
| 日图片上传 | 5,000 - 30,000 张 |

### 14.2 SLO

| 能力 | 目标 |
| --- | --- |
| API 可用性 | 月度 >= 99.9% |
| 核心写接口 P95 | <= 300ms |
| 核心读接口 P95 | <= 200ms |
| 后台统计接口 P95 | <= 1s |
| WebSocket 消息延迟 P95 | <= 1s |
| 事件消费延迟 P95 | <= 30s |
| RPO | <= 15 分钟 |
| RTO | <= 2 小时 |

### 14.3 高可用

- API 服务至少 2 个实例。
- Worker 可水平扩容。
- WebSocket 独立服务，支持多实例。
- PostgreSQL 使用高可用实例，必要时读写分离。
- 图片使用对象存储。
- Redis 用于限流、缓存、在线状态和短期锁。

---

## 15. 安全、隐私与风控

### 15.1 安全要求

- 所有接口必须鉴权，除登录、公开静态资源和必要回调外。
- 所有组内资源必须校验成员关系。
- 管理后台必须启用强密码和二次验证。
- 上传文件必须校验大小、类型、后缀、MIME 和内容。
- 日志禁止写入敏感明文。

### 15.2 隐私合规

- 最小化采集手机号、位置、图片等敏感数据。
- 提供注销、解绑、删除足迹、删除图片能力。
- 提供用户协议、隐私政策、儿童隐私条款。
- 地理位置默认可选。

### 15.3 风控

- 登录、注册、邀请、上传、下单、心愿选择、签到均需限流。
- 识别异常刷订单、刷积分、刷组经验、批量上传、恶意邀请。
- 每日爱心积分上限和每日组经验上限是全局防刷规则，所有订单类型都必须执行。
- 管理员补偿积分和发放钻石需保留审计。
- 钻石广告奖励预留反作弊校验。

---

## 16. 配置系统

所有配置均由管理员在后台维护。用户侧不提供订单积分、风控阈值、容量、奖励规则等配置入口。组级配置表示“管理员可按小组覆盖默认值”，不是组内用户自行配置。

### 16.1 全局配置

| 配置项 | 说明 |
| --- | --- |
| `sign_in_diamond_reward` | 单人签到组钻石奖励 |
| `full_team_sign_bonus` | 双方当日均签到额外奖励 |
| `wish_default_fulfillment_deadline_hours` | 心愿默认履约期限 |
| `wish_quality_reward_rules` | 心愿质量等级对应钻石奖励 |
| `admin_daily_diamond_limit` | 管理员单日发放钻石上限 |
| `swap_ignore_ongoing_wish` | 是否允许带在途心愿互换身份 |
| `normal_order_love_point_default` | 普通订单默认爱心积分 |
| `guest_order_love_point_default` | 做客订单默认爱心积分 |
| `normal_order_group_exp_default` | 普通订单默认组经验 |
| `guest_order_group_exp_default` | 做客订单默认组经验 |
| `daily_love_point_limit_default` | 用户每日爱心积分默认上限 |
| `daily_group_exp_limit_default` | 小组每日经验默认上限 |
| `group_level_exp_table` | 组等级所需经验配置 |
| `group_level_limit_bonus_table` | 组等级对应每日积分/经验上限加成 |
| `guest_order_daily_point_limit_per_guest` | 同一做客用户对同一主人家每日可计分订单上限 |
| `guest_invite_daily_point_limit` | 同一邀请链接每日可计分订单上限 |
| `guest_invite_total_point_limit` | 同一邀请链接总可计分订单上限 |
| `risk_reward_review_enabled` | 风险订单是否进入积分和经验人工审核 |

### 16.2 管理员组级覆盖配置

| 配置项 | 说明 |
| --- | --- |
| `normal_order_love_point` | 本组普通订单完成默认爱心积分 |
| `guest_order_love_point` | 本组做客订单完成默认爱心积分 |
| `normal_order_group_exp` | 本组普通订单完成默认组经验 |
| `guest_order_group_exp` | 本组做客订单完成默认组经验 |
| `daily_love_point_limit` | 本组用户每日爱心积分上限 |
| `daily_group_exp_limit` | 本组每日经验上限 |
| `order_timeout_hours` | 本组订单超时时间 |
| `food_capacity` | 菜品容量 |
| `tag_capacity` | 标签容量 |
| `footprint_capacity` | 足迹容量 |

---

## 17. 可观测性与运维

### 17.1 日志

- `access_log`：接口访问。
- `business_log`：关键业务操作。
- `audit_log`：后台和经济修复操作。
- `error_log`：错误和异常。

所有日志携带 `trace_id`。

### 17.2 指标

| 类别 | 指标 |
| --- | --- |
| API | QPS、错误率、P95/P99、慢接口 |
| 数据库 | 连接数、慢查询、锁等待、磁盘 |
| Worker | pending 数、消费延迟、失败数、死信数 |
| WebSocket | 在线连接、鉴权失败、消息失败 |
| 业务 | DAU、订单完成率、心愿选择率、角色互换次数、签到率 |

### 17.3 告警

- 核心 API 5 分钟错误率 > 2%。
- 数据库连接使用率 > 80%。
- Worker pending 持续增长超过 10 分钟。
- 经济对账不一致。
- WebSocket 异常断连突增。

### 17.4 发布

- 环境：dev、staging、production。
- 发布方式：灰度发布，5% -> 30% -> 全量。
- migration 需可回滚或提供补偿脚本。
- 核心功能使用 feature flag。
- 生产发布前必须完成冒烟测试。

---

## 18. 测试与验收

### 18.1 测试范围

| 类型 | 覆盖 |
| --- | --- |
| 单元测试 | 订单状态机、心愿状态机、积分冻结、角色互换 |
| 集成测试 | API + DB 事务、事件消费、通知、上传 |
| 并发测试 | 重复接单、重复确认、重复兑换、重复审核 |
| E2E 测试 | 登录、建组、下单、完成、积分、心愿、审核 |
| 安全测试 | 越权、JWT 篡改、上传绕过、后台权限 |
| 压力测试 | QPS、WebSocket、Worker 积压恢复 |

### 18.2 上线验收清单

- 双人组成员限制生效。
- Buyer/Seller 权限测试通过。
- 角色互换前置条件测试通过。
- 订单完成只给 Seller 发放爱心积分，并为小组增加经验。
- 每日爱心积分上限、每日组经验上限、超上限不发放奖励测试通过。
- 组等级升级扩大每日积分/经验上限测试通过。
- 用户侧不能配置订单积分，订单积分和风控阈值只能由管理员后台配置。
- 做客订单列表展示、主人家厨房菜单访问、做客下单和做客备注标记测试通过。
- 做客订单完成后给主人家 Seller 发放积分，做客用户不获得主人组积分。
- 订单刷积分/刷经验风控、风险订单人工审核、补偿扣回测试通过。
- 心愿协商积分和履约期限、双方线上确认、选择冻结、打卡扣减、逾期退还测试通过。
- 履约人与自然人绑定、角色切换不改变履约责任测试通过。
- 履约率展示和退出组前结清检查测试通过。
- 经济流水幂等测试通过。
- 组内数据隔离和越权测试通过。
- 监控、告警、备份、恢复演练完成。
- 隐私协议、用户注销、内容审核上线。
- 压测达到 10 万注册用户容量目标。

---

## 19. 推荐目录结构

```text
src/
  api/
    auth/
    groups/
    foods/
    orders/
    wishes/
    economy/
    sign_in/
    footprints/
    notifications/
    admin/
    ws/
  application/
    auth_service.rs
    group_service.rs
    order_service.rs
    wish_service.rs
    economy_service.rs
    event_handlers/
  domain/
    user/
    group/
    order/
    wish/
    economy/
    achievement/
  infrastructure/
    persistence/
    event/
    external/
    object_storage/
    cache/
  middlewares/
  utils/
  main.rs
```

---

## 20. 开发优先级

### P0

- 微信登录。
- 双人组与角色模型。
- 菜品管理。
- 订单状态机。
- 爱心积分、组经验、组钻石流水。
- 组等级与每日积分/经验上限。
- 心愿协商、选择、冻结、履约打卡、逾期退还、质量奖励。
- 权限、幂等、日志、基础通知。
- 管理后台基础审核和配置。

### P1

- 签到奖励。
- 足迹纪念。
- 成就系统。
- 做客账号与做客订单。
- 数据看板。
- 监控告警与灰度发布。

### P2

- 看广告获取钻石。
- 活动系统。
- AI 推荐。
- 更复杂的运营规则。

---

## 21. 当前已确认决策

1. 产品只面向固定双人组。
2. 资产合并为个人爱心积分 `love_point` 和组钻石 `diamond`。
3. 心愿模块按“用户创建、双方协商积分和履约期限、双方线上确认、进入心愿池、发起人攒积分选择、对方自然人履约、发起人打卡反馈、管理员按质量发放额外钻石”重构。
4. 做客用户需要长期账号，且可拥有自己的组。
5. 暂不做钻石充值，仅预留看广告获取钻石。
6. 无公开广场，内容仅组内显示。
7. 后端技术选型保持 Rust + PostgreSQL。
8. 10 万用户目标指注册用户。
9. 用户侧取消订单积分配置，所有配置相关能力全部交由管理员后台维护。
10. 做客用户通过邀请链接访问主人家厨房，看菜单、下单、备注标记；订单列表同时展示自己组订单和做客订单。
11. 做客订单由主人家完成后可给主人家 Seller 发放爱心积分，但必须接入防刷积分风控。
12. 防刷规则覆盖所有订单：用户每日爱心积分有上限，小组每日经验有上限，超出后订单完成但不再增加积分或经验。
13. 完成订单同样增加组经验并推动组升级，升级经验、每日经验上限和等级加成由管理员后台配置。
14. 心愿履约人与自然人用户 ID 绑定，和当前角色无关；逾期退还积分并记录履约数据，组内可查看履约率。
15. 用户退出组前必须结清未完结心愿、履约责任、冻结积分和相关补偿。

---

## 22. 关键实现提醒

- 心愿打卡反馈完成前不要正式扣除爱心积分，只冻结。
- 履约逾期、关闭都必须处理冻结积分退还。
- 角色切换只改变当前组角色映射，不修改历史记录。
- 已选择心愿的发起人和履约人永远按自然人用户 ID 绑定，角色切换不改变责任。
- Buyer 不通过订单获得爱心积分，这是轮换机制的核心约束。
- 每日积分/经验上限是订单奖励发放的硬约束，超上限订单照常完成但不发奖励。
- 同一组最多 2 名正式成员，这是权限、数据隔离和 UI 设计的基础假设。
- 所有经济、审核、角色互换操作都必须可审计、可追踪、可补偿。
