# 心愿菜单 - 生产级 API 设计文档

版本：2026-06-03
目标：10 万注册用户生产级产品
技术栈：Rust + PostgreSQL

---

## 1. 通用约定

### 1.1 基础规范

- **Base URL**: `/api`
- **认证方式**: JWT Bearer Token（登录后获取），短期访问令牌 + 可撤销刷新令牌
- **Content-Type**: `application/json; charset=utf-8`
- **时区**: 数据库 UTC 存储，接口返回 ISO 8601（e.g. `2026-06-03T08:00:00Z`）
- **分页**: 列表接口使用 cursor 分页，响应包含 `next_cursor`
- **幂等**: 所有写接口要求 Header `Idempotency-Key: <uuid>`

### 1.2 统一响应格式

```json
{
  "code": 0,
  "message": "success",
  "data": {},
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `code` | int | 0=成功，非0=失败 |
| `message` | string | 状态描述 |
| `data` | object/null | 响应数据 |
| `trace_id` | string | 链路追踪 ID |

### 1.3 统一错误响应

```json
{
  "code": 10001,
  "message": "LOVE_POINT_INSUFFICIENT",
  "data": {
    "required": 100,
    "available": 50,
    "frozen": 20
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### 1.4 通用错误码

| 错误码 | 说明 |
|--------|------|
| `AUTH_INVALID_TOKEN` | 登录态无效或已过期 |
| `AUTH_TOKEN_REVOKED` | 令牌已被撤销 |
| `PERMISSION_DENIED` | 无权限访问该资源 |
| `ROLE_NOT_ALLOWED` | 当前角色不允许操作 |
| `IDEMPOTENCY_CONFLICT` | 幂等键对应请求内容冲突 |
| `RESOURCE_NOT_FOUND` | 资源不存在 |
| `INVALID_PARAMETER` | 请求参数校验失败 |
| `INTERNAL_ERROR` | 服务器内部错误 |

---

## 2. 模块一：认证（auth）

### 2.1 微信登录

**接口**: `POST /api/auth/wechat-login`

**认证**: 否

**幂等**: 是（使用微信 code 作为幂等键）

**请求体**:
```json
{
  "code": "xxxxxxxxxxxxxxxxxxxx",
  "encrypted_data": "xxxxxxxxxxxxxxxxxxxx",
  "iv": "xxxxxxxxxxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `code` | string | 是 | 微信登录 code，从 `uni.login()` 获取 |
| `encrypted_data` | string | 否 | 微信加密数据（获取手机号时必填） |
| `iv` | string | 否 | 加密向量（获取手机号时必填） |

**响应体** (首次登录):
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "is_new_user": true,
    "user": {
      "id": 10001,
      "nickname": "",
      "avatar_url": "",
      "phone": null,
      "status": "ACTIVE"
    },
    "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
    "refresh_token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
    "expires_in": 7200
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**响应体** (老用户登录):
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "is_new_user": false,
    "user": {
      "id": 10001,
      "nickname": "小明",
      "avatar_url": "https://example.com/avatar.jpg",
      "phone": "138****8888",
      "status": "ACTIVE"
    },
    "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
    "refresh_token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
    "expires_in": 7200
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 前端不得直接传入可信 `openid` 创建正式用户
- 后端使用 `code` 换取微信 `openid` 和 `session_key`
- 日志中禁止记录 `code`、`session_key`、JWT 明文
- 新用户直接创建账号，昵称/头像可后续补充

**触发事件**: `UserLoggedInEvent`

---

### 2.2 刷新访问令牌

**接口**: `POST /api/auth/refresh`

**认证**: 否（使用 refresh_token）

**请求体**:
```json
{
  "refresh_token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
    "refresh_token": "yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy",
    "expires_in": 7200
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- refresh_token 一次性使用，用完立即失效并颁发新的 refresh_token
- 旧的 refresh_token 被使用后，旧 AccessToken 自动失效（会话挤占）

**错误码**:
- `AUTH_INVALID_TOKEN`: refresh_token 无效或已过期
- `AUTH_TOKEN_REVOKED`: 令牌已被撤销

---

### 2.3 注销登录

**接口**: `POST /api/auth/logout`

**认证**: 是

**请求体**: 无

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
- 使当前 refresh_token 失效
- 可选：使该用户所有令牌失效

---

## 3. 模块二：用户（user）

### 3.1 获取当前用户信息

**接口**: `GET /api/users/me`

**认证**: 是

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "id": 10001,
    "nickname": "小明",
    "avatar_url": "https://example.com/avatar.jpg",
    "phone": "138****8888",
    "status": "ACTIVE",
    "last_login_at": "2026-06-03T08:00:00Z",
    "created_at": "2026-06-01T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 3.2 更新用户资料

**接口**: `PATCH /api/users/me`

**认证**: 是

**请求体**:
```json
{
  "nickname": "小明",
  "avatar_url": "https://example.com/new-avatar.jpg",
  "phone": "13800138000"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `nickname` | string | 否 | 昵称，2-20字符 |
| `avatar_url` | string | 否 | 头像 URL |
| `phone` | string | 否 | 手机号（需要验证） |

**响应体**: 返回更新后的用户信息（同 3.1）

**业务规则**:
- 手机号修改需要短信验证码
- 头像上传走独立上传接口

---

### 3.3 获取用户的组列表

**接口**: `GET /api/users/me/groups`

**认证**: 是

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "groups": [
      {
        "group_id": 1001,
        "group_name": "甜蜜小屋",
        "my_role": "BUYER",
        "member_count": 2,
        "diamond_balance": 50,
        "level": 3,
        "joined_at": "2026-06-01T10:00:00Z"
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 3.4 账号注销

**接口**: `POST /api/users/me/delete`

**认证**: 是

**请求体**:
```json
{
  "reason": "不再使用"
}
```

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "deleted_at": "2026-06-03T08:00:00Z",
    "recovery_deadline": "2026-07-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 软删除，用户数据保留 30 天后彻底清除
- 注销前必须先退出所有组（通过结清检查）
- 注销期间新登录可以取消注销

---

## 4. 模块三：双人组（group）

### 4.1 创建双人组

**接口**: `POST /api/groups`

**认证**: 是

**幂等**: 是

**请求体**:
```json
{
  "name": "甜蜜小屋",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 组名，2-30字符 |
| `idempotency_key` | string | 是 | 幂等键 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "name": "甜蜜小屋",
    "buyer_user_id": 10001,
    "seller_user_id": 10002,
    "diamond_balance": 0,
    "level": 1,
    "exp": 0,
    "settings": {},
    "created_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 创建者默认为 Buyer，另一成员为 Seller
- 组内固定 2 人，创建时另一席位留空，等待邀请
- 新组默认等级 1，钻石 0，经验 0
- 每日积分/经验上限使用系统默认值

**触发事件**: `GroupCreatedEvent`

---

### 4.2 获取双人组详情

**接口**: `GET /api/groups/{group_id}`

**认证**: 是

**路径参数**:
| 字段 | 类型 | 说明 |
|------|------|------|
| `group_id` | integer | 小组 ID |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "name": "甜蜜小屋",
    "buyer_user_id": 10001,
    "seller_user_id": 10002,
    "diamond_balance": 50,
    "level": 5,
    "exp": 380,
    "daily_love_point_limit": 100,
    "daily_group_exp_limit": 200,
    "settings": {
      "swap_ignore_ongoing_wish": false
    },
    "status": "ACTIVE",
    "created_at": "2026-06-01T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 必须为组内成员才能查看

---

### 4.3 创建邀请链接

**接口**: `POST /api/groups/{group_id}/invite`

**认证**: 是

**幂等**: 是

**请求体**:
```json
{
  "expire_hours": 72,
  "max_uses": 1,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `expire_hours` | integer | 否 | 有效期（小时），默认 72，最大 168 |
| `max_uses` | integer | 否 | 最大使用次数，默认 1 |
| `idempotency_key` | string | 是 | 幂等键 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "invite_id": 1,
    "invite_code": "ABC123",
    "invite_url": "https://example.com/invite/ABC123",
    "expire_at": "2026-06-06T08:00:00Z",
    "max_uses": 1,
    "used_count": 0
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 仅 Seller 可创建邀请（Buyer 角色不能邀请）
- 邀请码 6 位字母数字组合
- 组满 2 人后邀请码自动失效

**触发事件**: `GroupInviteCreatedEvent`

---

### 4.4 加入双人组（通过邀请码）

**接口**: `POST /api/groups/join`

**认证**: 是

**幂等**: 是

**请求体**:
```json
{
  "invite_code": "ABC123",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "group_name": "甜蜜小屋",
    "my_role": "SELLER",
    "joined_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 受邀者自动成为 Seller
- 组满 2 人后拒绝加入
- 邀请码无效或已过期返回错误
- 每人最多拥有 5 个活跃组

**触发事件**: `GroupJoinedEvent`

---

### 4.5 角色互换

**接口**: `POST /api/groups/{group_id}/swap-role`

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
    "group_id": 1001,
    "old_buyer_user_id": 10001,
    "old_seller_user_id": 10002,
    "new_buyer_user_id": 10002,
    "new_seller_user_id": 10001,
    "swapped_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 前置条件：无未完结在途订单，操作人无 `CLAIMED` 状态且自己作为发起人或履约人的在途心愿
- 配置开关 `swap_ignore_ongoing_wish=true` 时，允许带在途心愿互换
- 历史订单、心愿、积分流水均保留操作时用户 ID 和当时角色快照

**错误码**:
- `ROLE_SWAP_BLOCKED_BY_ORDER`: 存在未完结订单
- `ROLE_SWAP_BLOCKED_BY_WISH`: 操作人存在在途心愿
- `PERMISSION_DENIED`: 无权限操作

**触发事件**: `RoleSwappedEvent`

---

### 4.6 退出双人组

**接口**: `POST /api/groups/{group_id}/exit`

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
  "data": null,
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 退出前必须通过结清检查（无未完结心愿、无冻结积分、无待处理责任）
- 退出后组内另一成员可继续使用，席位留空
- 组内只剩 1 人时组进入"待补充"状态
- 另一人可重新邀请新人加入

**错误码**:
- `GROUP_EXIT_SETTLEMENT_REQUIRED`: 有未结清订单/心愿/冻结积分

**触发事件**: `GroupExitedEvent`

---

### 4.7 退出组前结清检查

**接口**: `GET /api/groups/{group_id}/settlement-check`

**认证**: 是

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "can_exit": false,
    "pending_items": [
      {
        "type": "CLAIMED_WISH",
        "id": 101,
        "name": "一起看电影",
        "frozen_points": 50,
        "fulfiller_id": 10002,
        "deadline": "2026-06-10T20:00:00Z"
      },
      {
        "type": "ONGOING_ORDER",
        "id": 202,
        "status": "ACCEPTED",
        "title": "晚餐订单"
      }
    ],
    "summary": {
      "wish_count": 1,
      "order_count": 1,
      "total_frozen_points": 50
    }
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 检查当前用户作为发起人或履约人的未完结心愿
- 检查当前用户发起或接单的在途订单
- 检查当前用户在该组的冻结爱心积分
- 返回可读性摘要帮助用户理解需要处理的事项

---

### 4.8 获取组内成员列表

**接口**: `GET /api/groups/{group_id}/members`

**认证**: 是

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "members": [
      {
        "user_id": 10001,
        "nickname": "小明",
        "avatar_url": "https://example.com/avatar1.jpg",
        "my_role": "BUYER",
        "love_point_available": 120,
        "love_point_frozen": 30,
        "joined_at": "2026-06-01T10:00:00Z"
      },
      {
        "user_id": 10002,
        "nickname": "小红",
        "avatar_url": "https://example.com/avatar2.jpg",
        "my_role": "SELLER",
        "love_point_available": 80,
        "love_point_frozen": 0,
        "joined_at": "2026-06-02T10:00:00Z"
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 4.9 履约统计

**接口**: `GET /api/groups/{group_id}/fulfillment-stats`

**认证**: 是

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "users": [
      {
        "user_id": 10001,
        "nickname": "小明",
        "fulfillment_total": 15,
        "fulfillment_finished": 12,
        "fulfillment_expired": 2,
        "fulfillment_rate": 0.8,
        "avg_fulfillment_hours": 36.5,
        "pending_fulfillment_count": 1
      },
      {
        "user_id": 10002,
        "nickname": "小红",
        "fulfillment_total": 10,
        "fulfillment_finished": 10,
        "fulfillment_expired": 0,
        "fulfillment_rate": 1.0,
        "avg_fulfillment_hours": 24.0,
        "pending_fulfillment_count": 0
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 履约统计按自然人用户计算，与当前角色无关
- 只统计已结束（FINISHED/EXPIRED/CLOSED）的心愿
- 履约率 = 按期完成数 / 总履约数
- 平均履约时长从选择心愿到打卡确认的平均小时数

---

## 5. 模块四：菜品（food）

### 5.1 创建菜品

**接口**: `POST /api/groups/{group_id}/foods`

**认证**: 是（Seller 或组配置允许 Buyer）

**幂等**: 是

**请求体**:
```json
{
  "name": "红烧肉",
  "description": "妈妈的拿手菜",
  "images": [
    {
      "url": "https://example.com/food/1.jpg",
      "width": 800,
      "height": 600
    }
  ],
  "tags": ["肉类", "家常菜"],
  "ingredients": [
    {"name": "五花肉", "amount": "500g"},
    {"name": "冰糖", "amount": "30g"}
  ],
  "steps": [
    {"order": 1, "content": "五花肉切块焯水"},
    {"order": 2, "content": "锅中放油加冰糖炒色"},
    {"order": 3, "content": "加入肉块翻炒上色"}
  ],
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 菜品名称，2-50字符 |
| `description` | string | 否 | 描述，最多 500 字符 |
| `images` | array | 否 | 图片列表，最多 9 张 |
| `tags` | array | 否 | 标签，最多 5 个 |
| `ingredients` | array | 否 | 食材 |
| `steps` | array | 否 | 步骤 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "food_id": 1001,
    "group_id": 1001,
    "name": "红烧肉",
    "description": "妈妈的拿手菜",
    "images": [...],
    "tags": ["肉类", "家常菜"],
    "ingredients": [...],
    "steps": [...],
    "status": "ACTIVE",
    "created_by": 10001,
    "created_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 仅 Seller 可创建菜品（组配置允许时 Buyer 也可）
- 菜品容量受组等级限制（基础 20，超出需消耗钻石扩容）

**触发事件**: `FoodCreatedEvent`

---

### 5.2 获取菜品列表

**接口**: `GET /api/groups/{group_id}/foods`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20，最大 100 |
| `status` | string | 否 | 筛选状态：ACTIVE / HIDDEN / DELETED |
| `tags` | array | 否 | 按标签筛选 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "foods": [
      {
        "food_id": 1001,
        "name": "红烧肉",
        "description": "妈妈的拿手菜",
        "images": [...],
        "tags": ["肉类", "家常菜"],
        "status": "ACTIVE",
        "created_at": "2026-06-03T08:00:00Z"
      }
    ],
    "next_cursor": "eyJvZmZzZXQiOjIwfQ==",
    "has_more": true
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 5.3 获取菜品详情

**接口**: `GET /api/groups/{group_id}/foods/{food_id}`

**认证**: 是

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "food_id": 1001,
    "group_id": 1001,
    "name": "红烧肉",
    "description": "妈妈的拿手菜",
    "images": [...],
    "tags": ["肉类", "家常菜"],
    "ingredients": [
      {"name": "五花肉", "amount": "500g"},
      {"name": "冰糖", "amount": "30g"}
    ],
    "steps": [...],
    "status": "ACTIVE",
    "created_by": 10001,
    "updated_by": null,
    "created_at": "2026-06-03T08:00:00Z",
    "updated_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 5.4 更新菜品

**接口**: `PATCH /api/groups/{group_id}/foods/{food_id}`

**认证**: 是（Seller 或创建人）

**请求体**:
```json
{
  "name": "红烧肉（升级版）",
  "description": "增加了炖煮时间",
  "tags": ["肉类", "家常菜", "软糯"]
}
```

**响应体**: 返回更新后的菜品信息

**业务规则**:
- 仅 Seller 或菜品创建者可更新
- 更新需要记录 `updated_by`
- HIDDEN 状态的菜品不可被订单引用

**触发事件**: `FoodUpdatedEvent`

---

### 5.5 删除菜品

**接口**: `DELETE /api/groups/{group_id}/foods/{food_id}`

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
- 软删除，将 `status` 置为 `DELETED`
- 有进行中订单的菜品不可删除

---

### 5.6 隐藏/恢复菜品

**接口**: `POST /api/groups/{group_id}/foods/{food_id}/hide`

**认证**: 是

**请求体**:
```json
{
  "hidden": true
}
```

**业务规则**:
- HIDDEN 菜品不在菜单列表展示，但仍可被已有订单引用
- DELETED 菜品完全不可见

---

### 5.7 获取主人家厨房菜品列表（做客）

**接口**: `GET /api/kitchens/invitations/{invite_code}/foods`

**认证**: 是（受邀用户）

**响应体**: 同 5.2，但仅返回主人家 `ACTIVE` 状态的菜品

**业务规则**:
- 仅限持有有效邀请码的受邀用户访问
- 仅展示主人家组的菜品，与自己组隔离
- 不展示 HIDDEN 和 DELETED 菜品

---

## 6. 模块五：订单（order）

### 6.1 创建组内普通订单

**接口**: `POST /api/groups/{group_id}/orders`

**认证**: 是（Buyer）

**幂等**: 是

**状态机约束**: 无（新建订单）

**请求体**:
```json
{
  "food_id": 1001,
  "title": "晚餐红烧肉",
  "content": "少放糖，不要太甜",
  "deadline": "2026-06-03T19:00:00Z",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `food_id` | integer | 是 | 关联菜品 ID |
| `title` | string | 是 | 订单标题，2-50字符 |
| `content` | string | 否 | 详细需求，最多 500 字符 |
| `deadline` | string | 否 | 期望完成时间（组配置可定义默认超时） |
| `idempotency_key` | string | 是 | 幂等键 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10001,
    "group_id": 1001,
    "type": "NORMAL",
    "food_id": 1001,
    "title": "晚餐红烧肉",
    "content": "少放糖，不要太甜",
    "creator_id": 10001,
    "creator_role_snapshot": "BUYER",
    "assignee_id": 10002,
    "assignee_role_snapshot": "SELLER",
    "status": "CREATED",
    "love_point_reward": 10,
    "group_exp_reward": 5,
    "deadline": "2026-06-03T19:00:00Z",
    "risk_status": "PASS",
    "version": 1,
    "created_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 仅 Buyer 可创建组内订单
- 自动分配给 Seller
- 积分奖励值从组配置读取默认积分
- 创建时自动过风控，PASS 直接放行，SUSPECT/BLOCKED 需审核
- 不给 Buyer 发放爱心积分

**触发事件**: `OrderCreatedEvent`

---

### 6.2 创建做客订单

**接口**: `POST /api/kitchens/invitations/{invite_code}/orders`

**认证**: 是（受邀用户）

**幂等**: 是

**请求体**:
```json
{
  "food_id": 1001,
  "title": "来主人家想吃红烧肉",
  "content": "3人份，6点到",
  "guest_mark_tags": ["3人份", "6点到"],
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `food_id` | integer | 是 | 主人家菜品 ID |
| `title` | string | 是 | 订单标题 |
| `content` | string | 否 | 口味偏好、忌口、到访时间等备注 |
| `guest_mark_tags` | array | 否 | 订单标记标签 |
| `idempotency_key` | string | 是 | 幂等键 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10002,
    "group_id": 1001,
    "type": "GUEST",
    "food_id": 1001,
    "title": "来主人家想吃红烧肉",
    "content": "3人份，6点到",
    "guest_user_id": 10003,
    "guest_invite_id": 1,
    "guest_mark_tags": ["3人份", "6点到"],
    "creator_id": 10003,
    "assignee_id": 10002,
    "assignee_role_snapshot": "SELLER",
    "status": "CREATED",
    "love_point_reward": 10,
    "group_exp_reward": 5,
    "risk_status": "PASS",
    "version": 1,
    "created_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 仅持有有效邀请码的受邀用户可创建
- 订单归属主人家小组，由主人家 Seller 完成
- 主人家 Seller 完成后获得爱心积分
- 做客用户不获得主人组爱心积分
- 同一做客用户对同一主人家小组每日可计分做客订单不超过 1 单（风控）
- 创建时自动过风控

**触发事件**: `OrderCreatedEvent`（type=GUEST）

---

### 6.3 获取订单列表

**接口**: `GET /api/orders`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `type` | string | 否 | 订单类型：NORMAL / GUEST |
| `status` | string | 否 | 订单状态 |
| `group_id` | integer | 否 | 筛选特定组 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "orders": [
      {
        "order_id": 10001,
        "group_id": 1001,
        "group_name": "甜蜜小屋",
        "type": "NORMAL",
        "title": "晚餐红烧肉",
        "status": "CONFIRMED_COMPLETED",
        "creator_id": 10001,
        "assignee_id": 10002,
        "point_grant_status": "GRANTED",
        "exp_grant_status": "GRANTED",
        "created_at": "2026-06-03T08:00:00Z"
      },
      {
        "order_id": 10002,
        "group_id": 1001,
        "group_name": "甜蜜小屋",
        "type": "GUEST",
        "title": "来主人家想吃红烧肉",
        "status": "CREATED",
        "creator_id": 10003,
        "assignee_id": 10002,
        "guest_user_id": 10003,
        "guest_remark": "3人份，6点到",
        "point_grant_status": "NONE",
        "exp_grant_status": "NONE",
        "created_at": "2026-06-03T09:00:00Z"
      }
    ],
    "next_cursor": "eyJvZmZzZXQiOjIwfQ==",
    "has_more": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 聚合展示：自己所属组订单 + 自己发起的做客订单
- 做客订单标记主人家信息、订单类型 GUEST
- 仅展示当前用户有权限查看的订单

---

### 6.4 获取订单详情

**接口**: `GET /api/orders/{order_id}`

**认证**: 是

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10001,
    "group_id": 1001,
    "type": "NORMAL",
    "food_id": 1001,
    "food_name": "红烧肉",
    "title": "晚餐红烧肉",
    "content": "少放糖，不要太甜",
    "creator_id": 10001,
    "creator_role_snapshot": "BUYER",
    "assignee_id": 10002,
    "assignee_role_snapshot": "SELLER",
    "status": "CONFIRMED_COMPLETED",
    "love_point_reward": 10,
    "group_exp_reward": 5,
    "point_grant_status": "GRANTED",
    "exp_grant_status": "GRANTED",
    "risk_status": "PASS",
    "deadline": "2026-06-03T19:00:00Z",
    "accepted_at": "2026-06-03T08:30:00Z",
    "completed_at": "2026-06-03T18:45:00Z",
    "confirmed_at": "2026-06-03T19:00:00Z",
    "created_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 6.5 接单

**接口**: `POST /api/orders/{order_id}/accept`

**认证**: 是（Seller）

**幂等**: 是

**状态机约束**: 仅 `CREATED` 状态可接单

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
    "order_id": 10001,
    "status": "ACCEPTED",
    "assignee_id": 10002,
    "accepted_at": "2026-06-03T08:30:00Z",
    "version": 2
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 仅 Seller 可接单
- 使用条件更新确保仅 CREATED 状态可接单
- 接单后订单进入 ACCEPTED 状态

**错误码**:
- `ORDER_STATUS_INVALID`: 订单状态不允许接单
- `ROLE_NOT_ALLOWED`: 非 Seller 角色

**触发事件**: `OrderAcceptedEvent`

---

### 6.6 商家完成订单

**接口**: `POST /api/orders/{order_id}/complete`

**认证**: 是（Seller）

**幂等**: 是（`order_id + complete` 作为幂等键）

**状态机约束**: 仅 `ACCEPTED` 状态可完成

**请求体**:
```json
{
  "remark": "已完成摆盘",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10001,
    "status": "PRODUCTION_COMPLETED",
    "completed_at": "2026-06-03T18:45:00Z",
    "version": 3
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 仅 Seller 可完成订单
- 完成订单后爱心积分和组经验暂记，待 Buyer 确认后发放
- 过风控检查，超限则 point_grant_status=REJECTED_LIMIT

**触发事件**: `OrderProductionCompletedEvent`

---

### 6.7 买家确认订单

**接口**: `POST /api/orders/{order_id}/confirm`

**认证**: 是（Buyer）

**幂等**: 是（`order_id + confirm` 作为幂等键）

**状态机约束**: 仅 `PRODUCTION_COMPLETED` 状态可确认

**请求体**:
```json
{
  "type": "COMPLETE",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `type` | string | 是 | COMPLETE=完成，INCOMPLETE=未完成 |
| `idempotency_key` | string | 是 | 幂等键 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10001,
    "status": "CONFIRMED_COMPLETED",
    "confirmed_at": "2026-06-03T19:00:00Z",
    "point_grant_status": "GRANTED",
    "exp_grant_status": "GRANTED",
    "version": 4
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 仅 Buyer 可确认
- COMPLETE：正式发放爱心积分给 Seller，发放组经验给小组
- INCOMPLETE：订单关闭，不发放积分/经验
- 每日积分/经验上限检查，超限则状态 REJECTED_LIMIT 但订单仍完成
- 订单确认完成后写爱心积分流水、组经验流水
- 事件消费时生成足迹记录

**错误码**:
- `ORDER_STATUS_INVALID`: 状态不允许确认
- `DAILY_REWARD_LIMIT_REACHED`: 今日奖励已达上限（仍完成但不发放）

**触发事件**: `OrderConfirmedCompletedEvent`

---

### 6.8 取消订单

**接口**: `POST /api/orders/{order_id}/cancel`

**认证**: 是（Buyer 或 Seller）

**幂等**: 是

**状态机约束**: 仅 `CREATED` 状态可取消

**请求体**:
```json
{
  "reason": "不需要了",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10001,
    "status": "CANCELLED",
    "cancelled_at": "2026-06-03T08:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**触发事件**: `OrderCancelledEvent`

---

### 6.9 拒绝订单

**接口**: `POST /api/orders/{order_id}/reject`

**认证**: 是（Seller）

**幂等**: 是

**状态机约束**: 仅 `CREATED` 状态可拒绝

**请求体**:
```json
{
  "reason": "今天太忙了",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**: 同 6.8，status 变为 `REJECTED`

**触发事件**: `OrderRejectedEvent`

---

### 6.10 订单超时处理

**接口**: `POST /api/orders/{order_id}/timeout`

**认证**: 是（系统触发或管理员）

**状态机约束**: `CREATED` 或 `ACCEPTED` 状态超过组配置超时时间

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10001,
    "status": "TIMEOUT",
    "timeout_at": "2026-06-03T20:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 超时订单不发放积分/经验
- 生成超时记录用于统计

**触发事件**: `OrderTimeoutEvent`

---

### 6.11 更新做客订单备注

**接口**: `PATCH /api/orders/{order_id}/guest-remark`

**认证**: 是（做客用户）

**请求体**:
```json
{
  "content": "改到6点半到",
  "guest_mark_tags": ["6点半到"]
}
```

**响应体**: 返回更新后的订单信息

**业务规则**:
- 仅做客订单创建者可更新备注
- 仅在 CREATED / ACCEPTED 状态可更新

---

### 6.12 管理员审核风险订单

**接口**: `POST /api/admin/orders/{order_id}/reward-review`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:
```json
{
  "action": "APPROVE",
  "point_grant_status": "GRANTED",
  "exp_grant_status": "GRANTED",
  "remark": "审核通过",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `action` | string | 是 | APPROVE=批准，REJECT=拒绝 |
| `point_grant_status` | string | 否 | APPROVE时可选：GRANTED / REJECTED |
| `exp_grant_status` | string | 否 | APPROVE时可选：GRANTED / REJECTED |
| `remark` | string | 否 | 审核备注 |
| `idempotency_key` | string | 是 | 幂等键 |

**响应体**:
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "order_id": 10001,
    "point_grant_status": "GRANTED",
    "exp_grant_status": "GRANTED",
    "reviewer_id": 1,
    "reviewed_at": "2026-06-03T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:
- 管理员审核后触发积分/经验补偿流水（如批准且原本是 PENDING_REVIEW）
- 拒绝时原 PENDING_REVIEW 转为 REJECTED
- 补偿流水使用 `biz_type=ORDER_REWARD_COMPENSATION`
- 审核结果写入审计日志

**触发事件**: `OrderRewardReviewedEvent`

---

## 附录：订单状态机

```
CREATED
  ├── accept() ──> ACCEPTED
  ├── reject() ──> REJECTED
  ├── cancel() ──> CANCELLED
  └── timeout() ──> TIMEOUT

ACCEPTED
  ├── complete() ──> PRODUCTION_COMPLETED
  ├── cancel() ──> CANCELLED
  └── timeout() ──> TIMEOUT

PRODUCTION_COMPLETED
  ├── confirm(type=COMPLETE) ──> CONFIRMED_COMPLETED
  └── confirm(type=INCOMPLETE) ──> CONFIRMED_INCOMPLETE
```

---

**文档版本**: 2026-06-03
**状态**: 待审核