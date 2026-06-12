# 心愿菜单 - 生产级 API 设计文档（第二部分）

版本：2026-06-03
模块：wish / economy / sign_in / footprint / achievement

---

## 7. 模块六：心愿（wish）

### 7.1 创建心愿

**接口**: `POST /api/groups/{group_id}/wishes`

**认证**: 是（任一组内成员）

**幂等**: 是

**状态机约束**: 无（新建心愿）

**请求体**:

```json
{
  "name": "一起看电影",
  "description": "周末一起看场电影",
  "initial_cost": 50,
  "fulfillment_deadline_hours": 72,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段                         | 类型    | 必填 | 说明                                |
| ---------------------------- | ------- | ---- | ----------------------------------- |
| `name`                       | string  | 是   | 心愿名称，2-50字符                  |
| `description`                | text    | 否   | 心愿描述                            |
| `initial_cost`               | integer | 是   | 初始报价（爱心积分），需 > 0        |
| `fulfillment_deadline_hours` | integer | 否   | 履约期限（小时），默认 72，最大 720 |
| `idempotency_key`            | string  | 是   | 幂等键                              |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "group_id": 1001,
    "created_by": 10001,
    "requester_id": 10001,
    "fulfiller_id": 10002,
    "creator_role_snapshot": "BUYER",
    "name": "一起看电影",
    "description": "周末一起看场电影",
    "initial_cost": 50,
    "final_cost": null,
    "fulfillment_deadline_hours": 72,
    "status": "DRAFT",
    "selected_by": null,
    "selected_at": null,
    "fulfillment_due_at": null,
    "fulfilled_at": null,
    "version": 1,
    "created_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 创建者为 `requester_id`（发起人），另一成员自动为 `fulfiller_id`（履约人）
- 心愿角色与自然人用户 ID 绑定，与当前 Buyer/Seller 角色无关
- 创建后心愿处于 DRAFT 状态，双方需线上确认积分和期限后进入 CREATED

**触发事件**: `WishCreatedEvent`

---

### 7.2 获取心愿列表

**接口**: `GET /api/groups/{group_id}/wishes`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `status` | string | 否 | 状态：DRAFT / NEGOTIATING / CREATED / CLAIMED / FINISHED / EXPIRED / CLOSED |
| `role` | string | 否 | 按角色筛选：REQUESTER / FULFILLER |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wishes": [
      {
        "wish_id": 1001,
        "name": "一起看电影",
        "description": "周末一起看场电影",
        "final_cost": 50,
        "fulfillment_deadline_hours": 72,
        "status": "CREATED",
        "requester_id": 10001,
        "requester_nickname": "小明",
        "fulfiller_id": 10002,
        "fulfiller_nickname": "小红",
        "selected_by": null,
        "selected_at": null,
        "fulfillment_due_at": null,
        "created_at": "2026-06-03T08:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 7.3 获取心愿详情

**接口**: `GET /api/wishes/{wish_id}`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "group_id": 1001,
    "name": "一起看电影",
    "description": "周末一起看场电影",
    "initial_cost": 50,
    "final_cost": 50,
    "fulfillment_deadline_hours": 72,
    "status": "CLAIMED",
    "requester_id": 10001,
    "requester_nickname": "小明",
    "requester_role_snapshot": "BUYER",
    "fulfiller_id": 10002,
    "fulfiller_nickname": "小红",
    "fulfiller_role_snapshot": "SELLER",
    "selected_by": 10001,
    "selected_at": "2026-06-03T10:00:00Z",
    "fulfillment_due_at": "2026-06-06T10:00:00Z",
    "fulfilled_at": null,
    "quality_review_status": null,
    "quality_reviewer_id": null,
    "quality_remark": null,
    "diamond_reward": null,
    "version": 3,
    "created_at": "2026-06-03T08:00:00Z",
    "negotiations": [
      {
        "id": 1,
        "operator_id": 10001,
        "operator_role_snapshot": "BUYER",
        "action": "QUOTE",
        "cost": 50,
        "deadline_hours": 72,
        "created_at": "2026-06-03T08:05:00Z"
      },
      {
        "id": 2,
        "operator_id": 10002,
        "operator_role_snapshot": "SELLER",
        "action": "ACCEPT",
        "cost": 50,
        "deadline_hours": 72,
        "created_at": "2026-06-03T08:10:00Z"
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 7.4 心愿协商报价

**接口**: `POST /api/wishes/{wish_id}/quote`

**认证**: 是（requester 或 fulfiller）

**幂等**: 是

**状态机约束**: `DRAFT` 或 `NEGOTIATING` 状态

**请求体**:

```json
{
  "cost": 60,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型    | 必填 | 说明                     |
| ----------------- | ------- | ---- | ------------------------ |
| `cost`            | integer | 是   | 协商报价爱心积分，需 > 0 |
| `idempotency_key` | string  | 是   | 幂等键                   |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "status": "NEGOTIATING",
    "last_negotiation": {
      "action": "QUOTE",
      "cost": 60,
      "operator_id": 10001,
      "operator_role_snapshot": "BUYER",
      "created_at": "2026-06-03T08:15:00Z"
    },
    "version": 2
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 任一方可报价（QUOTE/COUNTER）
- 报价后心愿进入 NEGOTIATING 状态
- 报价仅记录本次协商记录，不改变 final_cost

**触发事件**: `WishNegotiatingEvent`

---

### 7.5 心愿协商履约期限

**接口**: `POST /api/wishes/{wish_id}/deadline`

**认证**: 是（requester 或 fulfiller）

**幂等**: 是

**状态机约束**: `DRAFT` 或 `NEGOTIATING` 状态

**请求体**:

```json
{
  "deadline_hours": 48,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**: 返回心愿详情（状态保持 NEGOTIATING）

**触发事件**: `WishNegotiatingEvent`

---

### 7.6 心愿双方线上确认

**接口**: `POST /api/wishes/{wish_id}/confirm-agreement`

**认证**: 是（requester 或 fulfiller）

**幂等**: 是

**状态机约束**: `NEGOTIATING` 状态

**请求体**:

```json
{
  "final_cost": 50,
  "fulfillment_deadline_hours": 48,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段                         | 类型    | 必填 | 说明                   |
| ---------------------------- | ------- | ---- | ---------------------- |
| `final_cost`                 | integer | 是   | 双方最终确认的爱心积分 |
| `fulfillment_deadline_hours` | integer | 是   | 履约期限（小时）       |
| `idempotency_key`            | string  | 是   | 幂等键                 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "status": "CREATED",
    "final_cost": 50,
    "fulfillment_deadline_hours": 48,
    "version": 3
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 双方都需要调用此接口确认（系统记录双方确认状态）
- 全部双方确认后心愿进入 CREATED（心愿池）
- 取消确认需要另一方先取消

**触发事件**: `WishAgreementConfirmedEvent`

---

### 7.7 心愿拒绝/关闭协商

**接口**: `POST /api/wishes/{wish_id}/reject`

**认证**: 是（requester 或 fulfiller）

**幂等**: 是

**状态机约束**: `DRAFT` 或 `NEGOTIATING` 状态

**请求体**:

```json
{
  "reason": "价格太高了",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "status": "CLOSED",
    "closed_at": "2026-06-03T08:30:00Z",
    "close_reason": "REJECTED",
    "close_remark": "价格太高了"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**触发事件**: `WishClosedEvent`

---

### 7.8 选择心愿并冻结积分

**接口**: `POST /api/wishes/{wish_id}/select`

**认证**: 是（requester，仅限发起人）

**幂等**: 是

**状态机约束**: `CREATED` 状态

**请求体**:

```json
{
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "status": "CLAIMED",
    "selected_by": 10001,
    "selected_at": "2026-06-03T10:00:00Z",
    "frozen_amount": 50,
    "fulfillment_due_at": "2026-06-05T10:00:00Z",
    "version": 4,
    "available_love_point_after": 70,
    "frozen_love_point_after": 50
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 仅发起人（requester）可选择心愿并冻结积分
- 选择时检查发起人爱心积分可用余额，余额不足拒绝
- 冻结成功后生成冻结流水（EARN_FREEZE），减少可用余额，增加冻结余额
- 同一时间同一心愿只能有 1 次有效选择
- 生成履约截止时间（选择时间 + 履约期限）
- 冻结积分不打正式扣减，等打卡反馈完成才扣减

**错误码**:

- `WISH_STATUS_INVALID`: 心愿状态不是 CREATED
- `LOVE_POINT_INSUFFICIENT`: 爱心积分不足

**触发事件**: `WishSelectedEvent`

---

### 7.9 提交打卡反馈

**接口**: `POST /api/wishes/{wish_id}/feedback`

**认证**: 是（requester，仅限发起人）

**幂等**: 是

**状态机约束**: `CLAIMED` 状态

**请求体**:

```json
{
  "content": "电影看完了！非常开心",
  "location": "CGV影城（万象城店）",
  "images": [
    {
      "url": "https://example.com/footprint/1.jpg",
      "width": 800,
      "height": 600
    }
  ],
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型   | 必填 | 说明                    |
| ----------------- | ------ | ---- | ----------------------- |
| `content`         | text   | 是   | 打卡内容，最多 500 字符 |
| `location`        | string | 否   | 位置（可选）            |
| `images`          | array  | 否   | 图片列表，最多 9 张     |
| `idempotency_key` | string | 是   | 幂等键                  |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "status": "FINISHED",
    "checkin_id": 1,
    "fulfilled_at": "2026-06-03T22:00:00Z",
    "frozen_amount_deducted": 50,
    "available_love_point_after": 70,
    "frozen_love_point_after": 0,
    "version": 5
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 仅发起人（requester）可提交打卡反馈
- 提交后冻结积分转正式扣减，生成扣减流水（DEDUCT）
- 心愿进入 FINISHED 终态
- 图片需要通过内容安全审核

**触发事件**: `WishFeedbackSubmittedEvent`, `WishFinishedEvent`

---

### 7.10 心愿逾期处理（系统/定时任务）

**接口**: `POST /api/wishes/{wish_id}/expire`

**认证**: 是（系统触发或管理员）

**状态机约束**: `CLAIMED` 状态且已超过履约截止时间

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "status": "EXPIRED",
    "expired_at": "2026-06-06T10:00:00Z",
    "frozen_amount_unfrozen": 50,
    "available_love_point_after": 120,
    "frozen_love_point_after": 0,
    "version": 5
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 履约人（Fulfiller）逾期未履约，退还冻结积分
- 生成解冻流水（UNFREEZE），退还冻结积分
- 记录履约人的逾期履约记录
- 心愿按配置回到 CREATED 或保持 EXPIRED 终态

**触发事件**: `WishExpiredEvent`

---

### 7.11 关闭心愿（双方协商一致）

**接口**: `POST /api/wishes/{wish_id}/close`

**认证**: 是（requester 或 fulfiller）

**幂等**: 是

**状态机约束**: 非 FINISHED / EXPIRED / CLOSED 终态

**请求体**:

```json
{
  "reason": "不想做了",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "status": "CLOSED",
    "closed_at": "2026-06-03T08:30:00Z",
    "close_reason": "USER_CLOSED",
    "frozen_amount_unfrozen": 0
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 任意非终态，双方协商一致可关闭
- 如存在冻结积分，必须解冻
- CLOSED 为终态，不可恢复

**触发事件**: `WishClosedEvent`

---

### 7.12 获取打卡记录列表

**接口**: `GET /api/wishes/{wish_id}/checkins`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "checkins": [
      {
        "id": 1,
        "wish_id": 1001,
        "content": "电影看完了！非常开心",
        "location": "CGV影城（万象城店）",
        "images": [...],
        "created_at": "2026-06-03T22:00:00Z"
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 7.13 管理员按反馈质量发放额外钻石奖励

**接口**: `POST /api/admin/wishes/{wish_id}/quality-reward`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "quality_level": "GOOD",
  "quality_remark": "反馈很详细",
  "diamond_reward": 5,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型    | 必填 | 说明                                       |
| ----------------- | ------- | ---- | ------------------------------------------ |
| `quality_level`   | string  | 是   | 质量等级：NONE / NORMAL / GOOD / EXCELLENT |
| `quality_remark`  | string  | 否   | 质量备注                                   |
| `diamond_reward`  | integer | 否   | 奖励钻石数量（可配置默认奖励）             |
| `idempotency_key` | string  | 是   | 幂等键                                     |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "wish_id": 1001,
    "quality_review_status": "REVIEWED",
    "quality_reviewer_id": 1,
    "quality_level": "GOOD",
    "quality_remark": "反馈很详细",
    "diamond_reward": 5,
    "quality_reviewed_at": "2026-06-04T10:00:00Z",
    "group_diamond_balance_after": 55
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 仅 FINISHED 状态心愿可发放质量奖励
- 钻石奖励必须走 diamond_transactions（type=EARN）
- 同一心愿额外钻石奖励必须幂等
- 可配置质量等级对应钻石、单次钻石上限和管理员单日发放上限
- 未发钻石时也可记录已查看质量，便于运营统计

**触发事件**: `WishQualityRewardedEvent`

---

## 附录：心愿状态机

```text
DRAFT
  ├── quote() ──> NEGOTIATING
  ├── deadline() ──> NEGOTIATING
  ├── reject() ──> CLOSED
  └── confirm-agreement()（单人）──> 保持 DRAFT（等待对方）

NEGOTIATING
  ├── quote() ──> NEGOTIATING
  ├── deadline() ──> NEGOTIATING
  ├── reject() ──> CLOSED
  └── confirm-agreement()（双方）──> CREATED

CREATED
  ├── select() ──> CLAIMED（冻结积分）
  ├── reject() ──> CLOSED
  └── expire()（系统）──> 保持 CREATED

CLAIMED
  ├── feedback() ──> FINISHED（正式扣减冻结积分）
  ├── expire()（系统）──> EXPIRED（解冻积分）
  └── close() ──> CLOSED（解冻积分）

FINISHED（终态）
  └── quality-reward()（管理员）──> FINISHED（可追加钻石）

EXPIRED（终态或按配置回到 CREATED）

CLOSED（终态）
```

---

## 8. 模块七：经济系统（economy）

### 8.1 获取当前用户积分余额

**接口**: `GET /api/groups/{group_id}/points/balance`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "user_id": 10001,
    "available_love_point": 120,
    "frozen_love_point": 30,
    "daily_love_point_limit": 100,
    "today_love_point_earned": 30,
    "today_love_point_remaining": 70
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 8.2 获取爱心积分流水

**接口**: `GET /api/groups/{group_id}/points/transactions`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `type` | string | 否 | 流水类型：EARN / FREEZE / UNFREEZE / DEDUCT / ADJUST |
| `biz_type` | string | 否 | 业务类型：ORDER_REWARD / WISH_FREEZE / WISH_DEDUCT / ADMIN_ADJUST |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "transactions": [
      {
        "id": 1001,
        "type": "EARN",
        "amount": 10,
        "available_before": 100,
        "available_after": 110,
        "frozen_before": 0,
        "frozen_after": 0,
        "biz_type": "ORDER_REWARD",
        "biz_id": 10001,
        "idempotency_key": "order-confirmed-10001",
        "created_at": "2026-06-03T19:00:00Z"
      },
      {
        "id": 1002,
        "type": "FREEZE",
        "amount": 50,
        "available_before": 120,
        "available_after": 70,
        "frozen_before": 0,
        "frozen_after": 50,
        "biz_type": "WISH_FREEZE",
        "biz_id": 1001,
        "idempotency_key": "wish-select-1001",
        "created_at": "2026-06-03T10:00:00Z"
      },
      {
        "id": 1003,
        "type": "DEDUCT",
        "amount": 50,
        "available_before": 70,
        "available_after": 70,
        "frozen_before": 50,
        "frozen_after": 0,
        "biz_type": "WISH_DEDUCT",
        "biz_id": 1001,
        "idempotency_key": "wish-feedback-1001",
        "created_at": "2026-06-03T22:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 8.3 获取小组钻石余额

**接口**: `GET /api/groups/{group_id}/diamonds/balance`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "diamond_balance": 50,
    "diamond_capacity": 100,
    "today_diamond_earned": 5
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 8.4 获取小组钻石流水

**接口**: `GET /api/groups/{group_id}/diamonds/transactions`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `type` | string | 否 | 流水类型：EARN / CONSUME / ADJUST |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "transactions": [
      {
        "id": 1,
        "type": "EARN",
        "amount": 5,
        "balance_before": 45,
        "balance_after": 50,
        "biz_type": "SIGN_IN_REWARD",
        "biz_id": null,
        "created_at": "2026-06-03T08:00:00Z"
      },
      {
        "id": 2,
        "type": "CONSUME",
        "amount": 10,
        "balance_before": 55,
        "balance_after": 45,
        "biz_type": "FOOTPRINT_CAPACITY_EXPANSION",
        "biz_id": null,
        "created_at": "2026-06-02T10:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 8.5 获取小组经验值和等级

**接口**: `GET /api/groups/{group_id}/exp`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "level": 5,
    "exp": 380,
    "next_level_exp": 500,
    "progress": 0.76,
    "daily_group_exp_limit": 200,
    "today_group_exp_earned": 50,
    "today_group_exp_remaining": 150
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 等级由管理员配置的等级经验表决定
- 升级后每日积分上限和每日经验上限扩大

---

### 8.6 获取小组经验流水

**接口**: `GET /api/groups/{group_id}/exp/transactions`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `type` | string | 否 | 流水类型：EARN / ADJUST / REVOKE |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "transactions": [
      {
        "id": 1,
        "type": "EARN",
        "amount": 5,
        "exp_before": 375,
        "exp_after": 380,
        "level_before": 5,
        "level_after": 5,
        "biz_type": "ORDER_COMPLETED",
        "biz_id": 10001,
        "created_at": "2026-06-03T19:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 8.7 管理员调整爱心积分（补偿）

**接口**: `POST /api/admin/groups/{group_id}/points/adjust`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "user_id": 10001,
  "type": "ADD",
  "amount": 20,
  "reason": "系统补偿",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型    | 必填 | 说明                  |
| ----------------- | ------- | ---- | --------------------- |
| `user_id`         | integer | 是   | 用户 ID               |
| `type`            | string  | 是   | ADD=增加，REDUCE=扣减 |
| `amount`          | integer | 是   | 数量                  |
| `reason`          | string  | 是   | 调整原因              |
| `idempotency_key` | string  | 是   | 幂等键                |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "user_id": 10001,
    "group_id": 1001,
    "type": "ADD",
    "amount": 20,
    "available_love_point_before": 120,
    "available_love_point_after": 140,
    "biz_type": "ADMIN_ADJUST",
    "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 所有经济修复必须走补偿流水，不允许后台直接改余额
- 补偿流水写入 love_point_transactions（type=ADJUST）
- 写入审计日志

**触发事件**: `LovePointAdjustedEvent`

---

## 9. 模块八：签到（sign_in）

### 9.1 当日签到

**接口**: `POST /api/groups/{group_id}/sign-in`

**认证**: 是

**幂等**: 是

**请求体**:

```json
{
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "sign_in_id": 1,
    "user_id": 10001,
    "group_id": 1001,
    "date": "2026-06-03",
    "reward_type": "SIGN_IN",
    "diamond_reward": 3,
    "consecutive_days": 5,
    "full_team_bonus": false,
    "group_diamond_balance_after": 53
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 同一天同一用户同一组只能签到一次
- 签到奖励发放组钻石
- 双方当日均签到时可触发额外组钻石奖励（full_team_bonus）
- 按用户本地时区计算连续天数，数据库统一 UTC 存储
- 签到后更新连续签到天数

**触发事件**: `SignInEvent`

---

### 9.2 获取签到记录

**接口**: `GET /api/groups/{group_id}/sign-ins`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `year_month` | string | 否 | 年月，如 2026-06 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "records": [
      {
        "date": "2026-06-03",
        "user_id": 10001,
        "consecutive_days": 5,
        "reward_type": "SIGN_IN",
        "diamond_reward": 3
      },
      {
        "date": "2026-06-02",
        "user_id": 10001,
        "consecutive_days": 4,
        "reward_type": "SIGN_IN",
        "diamond_reward": 2
      }
    ],
    "this_month_total": 15,
    "this_month_days": 3
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 9.3 获取组内双方签到状态

**接口**: `GET /api/groups/{group_id}/sign-in/status`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "date": "2026-06-03",
    "members": [
      {
        "user_id": 10001,
        "signed_in": true,
        "consecutive_days": 5
      },
      {
        "user_id": 10002,
        "signed_in": false,
        "consecutive_days": 3
      }
    ],
    "full_team_today": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

## 10. 模块九：足迹（footprint）

### 10.1 发布足迹

**接口**: `POST /api/groups/{group_id}/footprints`

**认证**: 是

**幂等**: 是

**请求体**:

```json
{
  "content": "今天一起做了红烧肉，幸福的味道",
  "location": "家里厨房",
  "images": [
    {
      "url": "https://example.com/footprint/1.jpg",
      "width": 800,
      "height": 600
    }
  ],
  "related_order_id": 10001,
  "related_wish_id": null,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段               | 类型    | 必填 | 说明                    |
| ------------------ | ------- | ---- | ----------------------- |
| `content`          | text    | 是   | 文字内容，最多 500 字符 |
| `location`         | string  | 否   | 可选位置                |
| `images`           | array   | 否   | 图片列表，最多 9 张     |
| `related_order_id` | integer | 否   | 关联订单 ID             |
| `related_wish_id`  | integer | 否   | 关联心愿 ID             |
| `idempotency_key`  | string  | 是   | 幂等键                  |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "footprint_id": 1,
    "group_id": 1001,
    "user_id": 10001,
    "content": "今天一起做了红烧肉，幸福的味道",
    "location": "家里厨房",
    "images": [...],
    "related_order_id": 10001,
    "related_wish_id": null,
    "created_at": "2026-06-03T20:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 足迹仅组内可见
- 图片需要通过内容安全审核
- 订单完成/心愿完成后可自动生成纪念足迹
- 足迹容量受组等级和钻石限制

**触发事件**: `FootprintCreatedEvent`

---

### 10.2 获取足迹列表

**接口**: `GET /api/groups/{group_id}/footprints`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `user_id` | integer | 否 | 按用户筛选 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "footprints": [
      {
        "footprint_id": 1,
        "user_id": 10001,
        "user_nickname": "小明",
        "user_avatar": "https://example.com/avatar1.jpg",
        "content": "今天一起做了红烧肉，幸福的味道",
        "location": "家里厨房",
        "images": [...],
        "related_order_id": 10001,
        "created_at": "2026-06-03T20:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false,
    "total_count": 25,
    "capacity": 50
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 10.3 删除足迹

**接口**: `DELETE /api/groups/{group_id}/footprints/{footprint_id}`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": null,
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 仅足迹创建者可删除
- 软删除或硬删除均可

> **V2.0 暂缓**：足迹评论和点赞功能（`POST/GET/DELETE /api/footprints/{id}/comments`、`POST/DELETE /api/footprints/{id}/like`）V1.0 不实现，FSD §24.11 已标记 V2.0 暂缓。v3.sql 中 `record_comment` / `record_like` 表已移除。

---

### 10.4 扩容足迹容量

**接口**: `POST /api/groups/{group_id}/footprints/capacity/expand`

**认证**: 是

**幂等**: 是

**请求体**:

```json
{
  "expand_by": 20,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型    | 必填 | 说明     |
| ----------------- | ------- | ---- | -------- |
| `expand_by`       | integer | 是   | 扩容数量 |
| `idempotency_key` | string  | 是   | 幂等键   |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "old_capacity": 50,
    "new_capacity": 70,
    "diamond_cost": 10,
    "diamond_balance_after": 40
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 消耗组钻石扩容
- 扩容费用按配置计算

---

## 11. 模块十：成就（achievement）

### 11.1 获取成就列表

**接口**: `GET /api/groups/{group_id}/achievements`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `category` | string | 否 | USER / GROUP |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "achievements": [
      {
        "achievement_id": "FIRST_ORDER",
        "name": "首单达成",
        "description": "完成你的第一个订单",
        "category": "USER",
        "icon": "https://example.com/achievement/first-order.png",
        "unlocked": true,
        "unlocked_at": "2026-06-01T10:00:00Z"
      },
      {
        "achievement_id": "LOVE_BIRD",
        "name": "甜蜜双飞",
        "description": "双人组累计完成 100 个订单",
        "category": "GROUP",
        "icon": "https://example.com/achievement/love-bird.png",
        "unlocked": false,
        "progress": 75,
        "total": 100
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 11.2 获取成就墙

**接口**: `GET /api/groups/{group_id}/achievements/wall`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "total_achievements": 20,
    "unlocked_count": 8,
    "achievements": [
      {
        "achievement_id": "FIRST_ORDER",
        "name": "首单达成",
        "category": "USER",
        "unlocked": true,
        "unlocked_at": "2026-06-01T10:00:00Z"
      }
    ],
    "next_unlock": {
      "achievement_id": "WEEK_STREAK",
      "name": "连续签到 7 天",
      "progress": 5,
      "total": 7
    }
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 11.3 成就解锁事件（系统内部）

**触发事件**: `AchievementUnlockedEvent`

**业务规则**:

- 成就由事件驱动异步判定
- 成就规则通过配置表管理
- 成就解锁必须幂等（通过流水唯一键防重）

---

**文档版本**: 2026-06-03
**状态**: 待审核
