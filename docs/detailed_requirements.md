# may_store 项目详细需求文档 (v2.2)

## 1. 项目概述

`may_store` 是一个专门为情侣、家庭或小型团队设计的协作化、游戏化生活管理平台。项目通过后端 Rust (Ntex) 服务，结合微信小程序，旨在通过“下单-接单”的互动模式、组级共享经济（钻石系统）、生活足迹记录以及成就系统，增进成员间的互动与情感连接。

## 2. 业务功能与流程说明

### 2.1 用户、组与权限管理

* **角色体系**:
  * `ORDERING` (吃货): 负责发起订单、兑换心愿。
  * `RECEIVING` (饲养员): 负责维护菜谱、接受并履行订单。
* **组模式**: 支持 `PAIR` (情侣) 等模式。核心经济资源（钻石）现在由组内成员共享。
* **角色互换**: 仅限 `PAIR` 组。校验当前组内无活跃订单（PENDING_ACCEPT, IN_PROGRESS 等）后完成角色互换。
* **对应表设计**:
  * `users`: 存储用户信息、爱心积分余额 (`love_point`)。
  * `association_groups`: 存储组基本信息、组共享钻石余额 (`diamond_balance`)。

### 2.2 菜单与菜品管理

* **业务流程**:
    1. **自治管理**: 下线“菜品审批”功能。`RECEIVING` 角色或管理员拥有对菜品、标签及食材的完全增删改查权限。
    2. **即时入库**: 新创建的菜品直接进入组共享库，无需等待审核。
    3. **盲盒抽取**: 组内成员可基于分类标签随机抽取正常状态的菜品。
* **对应表设计**:
  * `foods`: 存储菜品核心数据（制作步骤、图片、食材文本）。
  * `tags`: 定义组级菜品分类。
  * `ingredients`: 组级食材库，支持卡路里与单位管理。

### 2.3 订单生命周期与积分经济

* **业务流程**:
    1. **下单**: `ORDERING` 选择菜品，订单进入 `PENDING_ACCEPT`。
    2. **接单/超时**: `RECEIVING` 需在30分钟内接单。超时则自动扣除接单人积分。
    3. **交付/确认**: 接单方点击“完成”后，下单方点击“确认完成”，接单方获得个人积分奖励。
* **对应表设计**:
  * `orders`: 存储订单状态、积分奖励值。
  * `point_transactions`: 记录个人积分变动流水。

### 2.4 组钻石系统

* **业务流程**:
    1. **共享余额**: 钻石从个人属性迁移至组属性。组内任何成员的操作均会影响 `association_groups.diamond_balance`。
    2. **签到逻辑**:
        * 成员 A 签到：组钻石增加。
        * 成员 B 签到：组钻石再次增加。
        * **双人达成奖**: 若当日组内双方均完成签到，额外奖励组钻石（数额由 `group_point_configs` 配置）。
* **对应表设计**:
  * `association_groups`: 维护 `diamond_balance` 字段。
  * `diamond_flow`: 记录组级钻石的获取与消耗流水，记录操作人。
  * `sign_records`: 记录每个用户的签到日期，用于判断双人奖励触发。

### 2.5 足迹系统

* **业务流程**：
  1. **容量限制**：默认每组初始足迹容量上限为 **n 条**（建议默认 30，可由 `group_point_configs` 配置）。
      * 致命缺陷说明：当用户足迹已达上限时，若继续发布草稿，会导致发布失败、数据丢失或覆盖，严重影响用户体验。因此，必须在用户尝试发布前进行容量校验和友好提示。
      * 异常处理：后端接口需在容量已满时返回特定错误码（如 FOOTPRINT_CAPACITY_FULL），前端需拦截并弹窗提示。
  2. **扩容机制**：成员可消耗组钻石发起扩容，增加记录上限。扩容消耗数额建议默认 10 钻/10 条，可由管理员配置。
  3. **下单页面容量提示与解锁**：
      * 下单页面需实时显示“当前足迹容量（x/y）”，即已用/总容量。
      * 当容量已满时，默认关闭“完成后自动发布”。如用户选择“完成后自动发布”，前端需弹窗提示，并提供“解锁上限”按钮，支持用户在下单页面直接发起扩容（如弹窗或跳转支付流程）。
      * 接口建议：
        * GET /api/footprint/capacity 返回 { used: int, total: int }
        * POST /api/footprint/unlock_capacity { count: int } 解锁指定条数
  4. **自动发布控制**：
      * 订单需增加“是否完成后自动发布”字段（auto_publish_footprint: boolean），由用户下单时自主选择，前端本地存储用户偏好，后端订单表存储该字段。
      * 下单接口需支持该参数，后端据此决定订单完成后是否自动生成并发布足迹。
  5. **草稿发布奖励**：订单完成后自动生成的草稿发布时奖励组钻石（建议默认 1 钻/条）。
* **对应表设计**：
  * `user_record`: 核心足迹记录。
  * `association_groups`: 维护 `footprint_capacity` 字段。
  * `group_point_configs`: 配置单次扩容消耗的钻石数额。
  * `orders`: 增加 auto_publish_footprint 字段。
  
---

## 5. 主要表结构设计（核心字段）

### users

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 用户ID         |
| nickname       | VARCHAR(32)  | 昵称           |
| love_point     | INT          | 爱心积分       |
| group_id       | BIGINT       | 所属组         |

### association_groups

| 字段名           | 类型         | 说明           |
| ---------------- | ------------ | -------------- |
| id               | BIGSERIAL    | 组ID           |
| diamond_balance  | INT          | 钻石余额       |
| footprint_capacity | INT        | 足迹容量上限   |

### foods

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 菜品ID         |
| group_id       | BIGINT       | 所属组         |
| name           | VARCHAR(64)  | 菜品名         |
| steps          | TEXT         | 制作步骤       |
| image_url      | TEXT         | 图片           |

### orders

| 字段名                 | 类型         | 说明                       |
| ---------------------- | ------------ | -------------------------- |
| id                     | BIGSERIAL    | 订单ID                     |
| user_id                | BIGINT       | 下单用户                   |
| food_id                | BIGINT       | 菜品ID                     |
| status                 | VARCHAR(32)  | 订单状态                   |
| reward_point           | INT          | 完成奖励积分               |
| auto_publish_footprint | BOOLEAN      | 是否完成后自动发布足迹      |

### user_record

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 足迹ID         |
| group_id       | BIGINT       | 所属组         |
| user_id        | BIGINT       | 创建人         |
| content        | TEXT         | 足迹内容       |
| created_at     | TIMESTAMP    | 创建时间       |

### group_point_configs

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 配置ID         |
| group_id       | BIGINT       | 所属组         |
| footprint_capacity_default | INT | 默认足迹容量 |
| footprint_unlock_cost | INT   | 单次扩容消耗钻石 |

### diamond_flow

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 流水ID         |
| group_id       | BIGINT       | 所属组         |
| user_id        | BIGINT       | 操作人         |
| amount         | INT          | 变动数量       |
| reason         | VARCHAR(64)  | 变动原因       |
| created_at     | TIMESTAMP    | 时间           |

### point_transactions

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 流水ID         |
| user_id        | BIGINT       | 用户           |
| amount         | INT          | 变动数量       |
| reason         | VARCHAR(64)  | 变动原因       |
| created_at     | TIMESTAMP    | 时间           |

### sign_records

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 签到ID         |
| user_id        | BIGINT       | 用户           |
| group_id       | BIGINT       | 所属组         |
| sign_date      | DATE         | 签到日期       |

---

### 2.6 成就系统

成就系统用于量化“协作”与“陪伴”，激励用户持续活跃。分为三个核心维度：

#### 1. 成就类型与定义

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 成就ID         |
| code           | VARCHAR(32)  | 成就唯一标识   |
| name           | VARCHAR(64)  | 成就名称       |
| description    | TEXT         | 成就描述       |
| category       | VARCHAR(32)  | 分类（协作/情感/趣味）|
| threshold      | INT          | 达成阈值       |
| extra_data     | JSONB        | 其他参数       |

#### 2. 用户成就进度

| 字段名         | 类型         | 说明           |
| -------------- | ------------ | -------------- |
| id             | BIGSERIAL    | 记录ID         |
| user_id        | BIGINT       | 用户ID         |
| achievement_id | BIGINT       | 成就ID         |
| progress       | INT          | 当前进度       |
| unlocked_at    | TIMESTAMP    | 达成时间       |

#### 3. 业务处理流程

1. **成就定义**：所有成就预置于 `achievements` 表，支持后续扩展。
2. **进度追踪**：用户相关行为（如下单、签到、足迹发布等）触发进度更新，写入 `user_achievements`。
3. **达成判定**：每次进度变更后，若 progress >= threshold，记录达成时间并推送通知。
4. **前端展示**：前端通过接口获取所有成就定义及用户进度，分为“已达成/未达成”两类展示。

#### 4. 主要接口建议

* GET /api/achievement/list  
  返回所有成就定义及当前用户进度。
* POST /api/achievement/report_event  
  上报用户行为事件（如完成订单、签到等），后端自动处理进度。

#### 5. 典型成就示例

| code              | name         | description                                  | category | threshold | extra_data |
|-------------------|--------------|----------------------------------------------|----------|-----------|------------|
| order_100         | 米其林三星   | 累计完成100个订单且均为高分评价              | 协作     | 100       | {min_score:5} |
| quick_3_orders    | 绝佳默契     | 1小时内连续完成3个订单                       | 协作     | 3         | {time_limit:3600} |
| sign_7_days       | 心有灵犀     | 连续7天同一时间段签到                        | 协作     | 7         | {time_window:15} |
| footprint_500     | 时光收藏家   | 足迹扩容至500条                              | 情感     | 500       | {}         |
| group_999_days    | 长情陪伴     | 组建时间达到999天                            | 情感     | 999       | {}         |
| footprint_places  | 足迹遍布     | 在20个不同地点记录足迹                       | 情感     | 20        | {}         |
| night_orders      | 深夜食堂     | 凌晨0-4点完成订单                            | 趣味     | 1         | {time_range:[0,4]} |
| blindbox_20       | 盲盒达人     | 通过盲盒功能完成20种不同菜品                 | 趣味     | 20        | {}         |

#### 6. 业务处理说明

* 用户行为事件（如订单完成、签到、足迹发布等）由后端统一处理，自动更新相关成就进度。
* 达成成就后可推送通知、弹窗或奖励（如钻石、积分等）。
* 支持后续扩展更多成就类型和奖励。

---

## 3. 数据库模型总结

### 3.1 核心表关系

* **Group 核心**: `association_groups` <- 挂载 (`diamond_balance`, `footprint_capacity`, `point_configs`, `foods`, `records`)
* **User 核心**: `users` <- 挂载 (`love_point`, `sign_records`, `achievements`)

---

## 4. 技术实现

* **后端**: Rust (Ntex), Sqlx (PostgreSQL)。
* **自动化**: `Expiration Worker` 负责订单超时处理。
