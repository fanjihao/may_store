# 心愿菜单 - 生产级 API 设计文档（第三部分）

版本：2026-06-03
模块：notification / upload / admin / ws / dashboard

---

## 12. 模块十一：通知（notification）

### 12.1 获取通知列表

**接口**: `GET /api/notifications`

**认证**: 是

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `is_read` | boolean | 否 | 筛选已读/未读 |
| `type` | string | 否 | 通知类型：ORDER / WISH / SIGN_IN / SYSTEM |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "notifications": [
      {
        "id": 10001,
        "type": "ORDER",
        "title": "新订单",
        "content": "你有一个新的晚餐订单待接单",
        "data": {
          "order_id": 10001,
          "group_id": 1001
        },
        "is_read": false,
        "created_at": "2026-06-03T08:00:00Z"
      },
      {
        "id": 10002,
        "type": "WISH",
        "title": "心愿已确认",
        "content": "你的心愿「一起看电影」已被对方确认",
        "data": {
          "wish_id": 1001,
          "group_id": 1001
        },
        "is_read": true,
        "created_at": "2026-06-03T07:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 12.2 获取未读通知数

**接口**: `GET /api/notifications/unread-count`

**认证**: 是

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "total": 5,
    "by_type": {
      "ORDER": 2,
      "WISH": 1,
      "SIGN_IN": 1,
      "SYSTEM": 1
    }
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 12.3 标记单条通知为已读

**接口**: `POST /api/notifications/{notification_id}/read`

**认证**: 是

**幂等**: 是

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

---

### 12.4 批量标记通知为已读

**接口**: `POST /api/notifications/read-all`

**认证**: 是

**幂等**: 是

**请求体**:

```json
{
  "notification_ids": [10001, 10002, 10003]
}
```

| 字段               | 类型  | 必填 | 说明                         |
| ------------------ | ----- | ---- | ---------------------------- |
| `notification_ids` | array | 否   | 通知 ID 数组，不传则全部标记 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "updated_count": 3
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 12.5 删除通知

**接口**: `DELETE /api/notifications/{notification_id}`

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

---

## 13. 模块十二：文件上传（upload）

### 13.1 获取预签名上传 URL

**接口**: `POST /api/uploads/presigned-url`

**认证**: 是

**幂等**: 是

**请求体**:

```json
{
  "filename": "photo.jpg",
  "content_type": "image/jpeg",
  "size": 1024000,
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型    | 必填 | 说明                                  |
| ----------------- | ------- | ---- | ------------------------------------- |
| `filename`        | string  | 是   | 原始文件名                            |
| `content_type`    | string  | 是   | MIME 类型                             |
| `size`            | integer | 是   | 文件大小（字节），最大 5242880（5MB） |
| `idempotency_key` | string  | 是   | 幂等键                                |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "upload_url": "https://cdn.example.com/upload?signature=eyJ...",
    "file_key": "uploads/2026/06/03/abc123def456.jpg",
    "expires_at": "2026-06-03T12:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 图片限制：JPEG/PNG/GIF，最大 5MB
- 预签名 URL 有效期 30 分钟
- 直接上传到对象存储，不经过业务服务器

**错误码**:

- `UPLOAD_SIZE_EXCEEDED`: 文件大小超出 5MB
- `UPLOAD_TYPE_NOT_ALLOWED`: 不支持的文件类型

---

### 13.2 确认上传完成

**接口**: `POST /api/uploads/confirm`

**认证**: 是

**幂等**: 是

**请求体**:

```json
{
  "file_key": "uploads/2026/06/03/abc123def456.jpg"
}
```

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "file_key": "uploads/2026/06/03/abc123def456.jpg",
    "cdn_url": "https://cdn.example.com/uploads/2026/06/03/abc123def456.jpg",
    "content_check_status": "PENDING"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**content_check_status 取值**:

- `PASS`: 内容安全检查通过
- `PENDING`: 检查中，稍后回调
- `REJECTED`: 内容安全检查未通过

**业务规则**:

- 上传后需确认才返回可用的 CDN URL
- 内容安全审核异步进行
- 审核未通过时返回 `UPLOAD_CONTENT_REJECTED`

---

### 13.3 批量获取预签名 URL

**接口**: `POST /api/uploads/presigned-urls`

**认证**: 是

**幂等**: 是

**请求体**:

```json
{
  "files": [
    { "filename": "photo1.jpg", "content_type": "image/jpeg", "size": 1024000 },
    { "filename": "photo2.jpg", "content_type": "image/jpeg", "size": 2048000 }
  ],
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "uploads": [
      {
        "filename": "photo1.jpg",
        "upload_url": "https://cdn.example.com/upload?signature=xxx1",
        "file_key": "uploads/2026/06/03/file1.jpg",
        "expires_at": "2026-06-03T12:00:00Z"
      },
      {
        "filename": "photo2.jpg",
        "upload_url": "https://cdn.example.com/upload?signature=xxx2",
        "file_key": "uploads/2026/06/03/file2.jpg",
        "expires_at": "2026-06-03T12:00:00Z"
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 单次最多 9 个文件
- 总大小不超过 20MB

---

### 13.4 删除上传文件

**接口**: `DELETE /api/uploads/{file_key:path}`

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

- 删除后 CDN URL 立即失效
- 已在业务中使用的图片不建议删除

---

## 14. 模块十三：WebSocket 实时通知（ws）

### 14.1 连接与鉴权

**连接 URL**: `wss://api.example.com/ws`

**连接后 10 秒内必须发送鉴权帧**:

**客户端发送（鉴权帧）**:

```json
{
  "type": "auth",
  "token": "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "device_id": "device-uuid-xxxxx"
}
```

**服务端响应（成功）**:

```json
{
  "type": "auth_ok",
  "user_id": 10001,
  "groups": [1001, 1002],
  "heartbeat_interval": 30,
  "server_time": "2026-06-03T08:00:00Z"
}
```

**服务端响应（失败）**:

```json
{
  "type": "auth_failed",
  "code": "AUTH_INVALID_TOKEN",
  "message": "Token 无效或已过期"
}
```

**业务规则**:

- 鉴权失败后连接立即断开
- 多实例场景使用 Redis 维护用户在线状态

---

### 14.2 心跳

**客户端发送（ping）**:

```json
{
  "type": "ping",
  "timestamp": 1709452800000
}
```

**服务端响应（pong）**:

```json
{
  "type": "pong",
  "server_time": "2026-06-03T08:00:30Z"
}
```

**业务规则**:

- 心跳间隔 30 秒
- 客户端超过 90 秒未发心跳，服务端主动断开连接

---

### 14.3 服务端推送：通知消息

**服务端发送**:

```json
{
  "type": "notification",
  "data": {
    "id": 10001,
    "notification_type": "ORDER",
    "title": "订单已接单",
    "content": "对方已接下你的晚餐订单",
    "data": {
      "order_id": 10001,
      "group_id": 1001
    }
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000",
  "sent_at": "2026-06-03T08:30:00Z"
}
```

---

### 14.4 服务端推送：订单状态变更

**服务端发送**:

```json
{
  "type": "order_status_changed",
  "data": {
    "order_id": 10001,
    "group_id": 1001,
    "status": "ACCEPTED",
    "old_status": "CREATED",
    "updated_by": 10002,
    "updated_at": "2026-06-03T08:30:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000",
  "sent_at": "2026-06-03T08:30:00Z"
}
```

---

### 14.5 服务端推送：心愿状态变更

**服务端发送**:

```json
{
  "type": "wish_status_changed",
  "data": {
    "wish_id": 1001,
    "group_id": 1001,
    "status": "CLAIMED",
    "old_status": "CREATED",
    "selected_by": 10001,
    "updated_at": "2026-06-03T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000",
  "sent_at": "2026-06-03T10:00:00Z"
}
```

---

### 14.6 服务端推送：积分变动

**服务端发送**:

```json
{
  "type": "love_point_changed",
  "data": {
    "user_id": 10001,
    "group_id": 1001,
    "change_type": "EARN",
    "amount": 10,
    "available_after": 130,
    "frozen_after": 0
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000",
  "sent_at": "2026-06-03T19:00:00Z"
}
```

---

### 14.7 服务端推送：履约提醒

**服务端发送**:

```json
{
  "type": "wish_fulfill_reminder",
  "data": {
    "wish_id": 1001,
    "group_id": 1001,
    "name": "一起看电影",
    "fulfillment_due_at": "2026-06-06T10:00:00Z",
    "remaining_hours": 24
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000",
  "sent_at": "2026-06-05T10:00:00Z"
}
```

---

### 14.8 客户端发送：订阅组消息

**客户端发送**:

```json
{
  "type": "subscribe",
  "groups": [1001, 1002]
}
```

**服务端响应**:

```json
{
  "type": "subscribed",
  "groups": [1001, 1002]
}
```

**业务规则**:

- 连接鉴权后默认订阅用户所在的所有组
- 可通过此接口动态订阅/取消订阅组

---

## 15. 模块十四：管理后台（admin）

### 15.1 获取用户列表

**接口**: `GET /api/admin/users`

**认证**: 是（管理员）

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `status` | string | 否 | 筛选状态：ACTIVE / BANNED / DELETED |
| `keyword` | string | 否 | 搜索昵称或手机号 |
| `group_id` | integer | 否 | 筛选特定组 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "users": [
      {
        "user_id": 10001,
        "openid": "oXXXX",
        "nickname": "小明",
        "avatar_url": "https://example.com/avatar.jpg",
        "phone": "138****8888",
        "status": "ACTIVE",
        "group_count": 1,
        "last_login_at": "2026-06-03T08:00:00Z",
        "created_at": "2026-06-01T10:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false,
    "total_count": 10500
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 15.2 获取用户详情

**接口**: `GET /api/admin/users/{user_id}`

**认证**: 是（管理员）

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "user_id": 10001,
    "openid": "oXXXX",
    "nickname": "小明",
    "avatar_url": "https://example.com/avatar.jpg",
    "phone": "138****8888",
    "status": "ACTIVE",
    "groups": [
      {
        "group_id": 1001,
        "group_name": "甜蜜小屋",
        "my_role": "BUYER",
        "joined_at": "2026-06-01T10:00:00Z"
      }
    ],
    "love_point_summary": {
      "total_available": 120,
      "total_frozen": 30
    },
    "fulfillment_stats": {
      "total_wishes": 15,
      "finished_wishes": 12,
      "expired_wishes": 2,
      "fulfillment_rate": 0.8
    },
    "last_login_at": "2026-06-03T08:00:00Z",
    "created_at": "2026-06-01T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 15.3 禁用/解禁用户

**接口**: `POST /api/admin/users/{user_id}/status`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "status": "BANNED",
  "reason": "刷单违规",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型   | 必填 | 说明            |
| ----------------- | ------ | ---- | --------------- |
| `status`          | string | 是   | ACTIVE / BANNED |
| `reason`          | string | 否   | 操作原因        |
| `idempotency_key` | string | 是   | 幂等键          |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "user_id": 10001,
    "status": "BANNED",
    "banned_at": "2026-06-03T10:00:00Z",
    "ban_reason": "刷单违规"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 禁用后用户无法登录
- 禁用用户的订单/心愿保持不变，待处理
- 写入审计日志

---

### 15.4 获取小组列表

**接口**: `GET /api/admin/groups`

**认证**: 是（管理员）

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `status` | string | 否 | 筛选状态：ACTIVE / INACTIVE |
| `level_min` | integer | 否 | 最低等级 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "groups": [
      {
        "group_id": 1001,
        "name": "甜蜜小屋",
        "level": 5,
        "member_count": 2,
        "diamond_balance": 50,
        "exp": 380,
        "status": "ACTIVE",
        "created_at": "2026-06-01T10:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false,
    "total_count": 5200
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 15.5 获取组详情

**接口**: `GET /api/admin/groups/{group_id}`

**认证**: 是（管理员）

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group_id": 1001,
    "name": "甜蜜小屋",
    "level": 5,
    "exp": 380,
    "diamond_balance": 50,
    "settings": {
      "swap_ignore_ongoing_wish": false
    },
    "status": "ACTIVE",
    "members": [...],
    "stats": {
      "total_orders": 120,
      "completed_orders": 115,
      "total_wishes": 25,
      "finished_wishes": 20
    },
    "created_at": "2026-06-01T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 15.6 更新组级配置

**接口**: `PATCH /api/admin/groups/{group_id}/configs`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "normal_order_love_point": 15,
  "guest_order_love_point": 12,
  "normal_order_group_exp": 8,
  "guest_order_group_exp": 6,
  "daily_love_point_limit": 150,
  "daily_group_exp_limit": 300,
  "food_capacity": 50,
  "footprint_capacity": 100,
  "order_timeout_hours": 48,
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
    "updated_configs": {
      "normal_order_love_point": 15,
      "daily_love_point_limit": 150
    },
    "updated_at": "2026-06-03T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 组级配置是管理员可按小组覆盖默认值的机制
- 变更写入审计日志

---

### 15.7 获取全局配置

**接口**: `GET /api/admin/configs`

**认证**: 是（管理员）

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `category` | string | 否 | 配置分类：REWARDS / SIGN_IN / WISH / ORDER / RISK |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "configs": [
      {
        "key": "sign_in_diamond_reward",
        "value": 5,
        "type": "INT",
        "description": "单人签到组钻石奖励",
        "category": "SIGN_IN",
        "updated_at": "2026-06-01T00:00:00Z"
      },
      {
        "key": "full_team_sign_bonus",
        "value": 3,
        "type": "INT",
        "description": "双方当日均签到额外奖励",
        "category": "SIGN_IN",
        "updated_at": "2026-06-01T00:00:00Z"
      },
      {
        "key": "daily_love_point_limit_default",
        "value": 100,
        "type": "INT",
        "description": "用户每日爱心积分默认上限",
        "category": "REWARDS",
        "updated_at": "2026-06-01T00:00:00Z"
      },
      {
        "key": "group_level_exp_table",
        "value": "{\"1\":0,\"2\":100,\"3\":300,\"4\":600,\"5\":1000}",
        "type": "JSON",
        "description": "组等级所需经验配置",
        "category": "REWARDS",
        "updated_at": "2026-06-01T00:00:00Z"
      },
      {
        "key": "wish_quality_reward_rules",
        "value": "{\"NONE\":0,\"NORMAL\":2,\"GOOD\":5,\"EXCELLENT\":10}",
        "type": "JSON",
        "description": "心愿质量等级对应钻石奖励",
        "category": "WISH",
        "updated_at": "2026-06-01T00:00:00Z"
      },
      {
        "key": "admin_daily_diamond_limit",
        "value": 100,
        "type": "INT",
        "description": "管理员单日发放钻石上限",
        "category": "WISH",
        "updated_at": "2026-06-01T00:00:00Z"
      },
      {
        "key": "risk_reward_review_enabled",
        "value": true,
        "type": "BOOL",
        "description": "风险订单是否进入积分和经验人工审核",
        "category": "RISK",
        "updated_at": "2026-06-01T00:00:00Z"
      }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 15.8 更新全局配置

**接口**: `PATCH /api/admin/configs/{config_key}`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "value": 10,
  "reason": "调整签到奖励",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "key": "sign_in_diamond_reward",
    "old_value": 5,
    "new_value": 10,
    "updated_at": "2026-06-03T10:00:00Z",
    "updated_by": 1
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 配置变更必须写入审计日志
- 敏感配置变更需二次确认

---

### 15.9 管理员补偿积分（经济修复）

**接口**: `POST /api/admin/groups/{group_id}/points/compensate`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "user_id": 10001,
  "type": "ADD",
  "amount": 50,
  "biz_type": "SYSTEM_COMPENSATION",
  "remark": "系统漏发订单奖励补偿",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段              | 类型    | 必填 | 说明                             |
| ----------------- | ------- | ---- | -------------------------------- |
| `user_id`         | integer | 是   | 用户 ID                          |
| `type`            | string  | 是   | ADD=增加，REDUCE=扣减            |
| `amount`          | integer | 是   | 数量                             |
| `biz_type`        | string  | 是   | SYSTEM_COMPENSATION / ADMIN_GIFT |
| `remark`          | string  | 是   | 补偿说明（写入审计）             |
| `idempotency_key` | string  | 是   | 幂等键                           |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "user_id": 10001,
    "group_id": 1001,
    "type": "ADD",
    "amount": 50,
    "biz_type": "SYSTEM_COMPENSATION",
    "transaction_id": 10001,
    "available_love_point_before": 70,
    "available_love_point_after": 120,
    "compensated_by": 1,
    "compensated_at": "2026-06-03T10:00:00Z"
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**业务规则**:

- 经济修复必须走补偿流水
- 禁止后台直接改余额
- 补偿流水写入 love_point_transactions（type=ADJUST）
- 写入审计日志（操作人、操作原因、补偿前后余额）

---

### 15.10 管理员补偿钻石

**接口**: `POST /api/admin/groups/{group_id}/diamonds/compensate`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "type": "ADD",
  "amount": 20,
  "remark": "活动奖励补偿",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**响应体**: 同 15.9 结构

**业务规则**:

- 补偿流水写入 diamond_transactions（type=ADJUST）
- 写入审计日志

---

### 15.11 获取待审核订单列表

**接口**: `GET /api/admin/orders/pending-review`

**认证**: 是（管理员）

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 20 |
| `risk_status` | string | 否 | 风险状态：SUSPECT / BLOCKED |

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
        "type": "GUEST",
        "creator_id": 10003,
        "status": "CONFIRMED_COMPLETED",
        "risk_status": "SUSPECT",
        "risk_detail": {
          "reason": "同一 IP 短时间内大量下单",
          "rules_hit": ["IP_FREQUENCY", "GUEST_FREQUENCY"]
        },
        "love_point_reward": 10,
        "group_exp_reward": 5,
        "point_grant_status": "PENDING_REVIEW",
        "exp_grant_status": "PENDING_REVIEW",
        "created_at": "2026-06-03T08:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false,
    "total_count": 12
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 15.12 审核风险订单

**接口**: `POST /api/admin/orders/{order_id}/review`

**认证**: 是（管理员）

**幂等**: 是

**请求体**:

```json
{
  "action": "APPROVE",
  "point_grant_status": "GRANTED",
  "exp_grant_status": "GRANTED",
  "remark": "核实为正常消费",
  "idempotency_key": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

| 字段                 | 类型   | 必填 | 说明                                |
| -------------------- | ------ | ---- | ----------------------------------- |
| `action`             | string | 是   | APPROVE=批准发放，REJECT=拒绝发放   |
| `point_grant_status` | string | 否   | APPROVE时可设置：GRANTED / REJECTED |
| `exp_grant_status`   | string | 否   | APPROVE时可设置：GRANTED / REJECTED |
| `remark`             | string | 否   | 审核备注                            |
| `idempotency_key`    | string | 是   | 幂等键                              |

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

- APPROVE 后补发爱心积分和组经验流水
- REJECT 后原 PENDING_REVIEW 转为 REJECTED，不发放
- 审核结果写入审计日志

---

### 15.13 获取审计日志

**接口**: `GET /api/admin/audit-logs`

**认证**: 是（管理员）

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cursor` | string | 否 | 游标分页 |
| `limit` | integer | 否 | 每页数量，默认 50 |
| `operator_id` | integer | 否 | 操作人筛选 |
| `action_type` | string | 否 | 操作类型：USER_BAN / CONFIG_UPDATE / POINT_COMPENSATE 等 |
| `start_date` | string | 否 | 开始日期 |
| `end_date` | string | 否 | 结束日期 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "logs": [
      {
        "id": 1,
        "operator_id": 1,
        "operator_nickname": "管理员",
        "action_type": "POINT_COMPENSATE",
        "target_type": "USER",
        "target_id": 10001,
        "detail": {
          "amount": 50,
          "reason": "系统补偿"
        },
        "ip": "10.0.0.1",
        "created_at": "2026-06-03T10:00:00Z"
      }
    ],
    "next_cursor": null,
    "has_more": false
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

## 16. 模块十五：数据看板（dashboard）

### 16.1 用户组内看板

**接口**: `GET /api/groups/{group_id}/dashboard`

**认证**: 是（组内成员）

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "group": {
      "group_id": 1001,
      "name": "甜蜜小屋",
      "level": 5,
      "exp": 380,
      "next_level_exp": 500,
      "diamond_balance": 50,
      "daily_love_point_limit": 100,
      "daily_group_exp_limit": 200
    },
    "today": {
      "date": "2026-06-03",
      "orders_completed": 2,
      "love_points_earned": 20,
      "group_exp_earned": 10,
      "sign_in": {
        "user_10001": { "signed": true, "consecutive_days": 5 },
        "user_10002": { "signed": false, "consecutive_days": 3 }
      }
    },
    "this_month": {
      "orders_completed": 45,
      "wishes_finished": 8,
      "love_points_spent": 300,
      "love_points_earned": 450
    },
    "quick_stats": {
      "total_orders": 120,
      "total_wishes": 25,
      "finished_wishes": 20,
      "fulfillment_rate": 0.8,
      "continuous_sign_in_days_user1": 5,
      "continuous_sign_in_days_user2": 3
    }
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 16.2 管理员运营看板

**接口**: `GET /api/admin/dashboard`

**认证**: 是（管理员）

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `start_date` | string | 否 | 开始日期，默认当天 |
| `end_date` | string | 否 | 结束日期，默认当天 |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "overview": {
      "total_users": 10500,
      "total_groups": 5200,
      "dau": 3200,
      "new_users_today": 150,
      "new_groups_today": 75,
      "active_guest_orders_today": 320
    },
    "order_stats": {
      "total_orders_today": 8500,
      "completed_orders_today": 7800,
      "completion_rate": 0.918,
      "avg_completion_hours": 2.3,
      "guest_orders_today": 320,
      "normal_orders_today": 8180
    },
    "wish_stats": {
      "total_wishes": 3200,
      "active_wishes": 450,
      "finished_wishes": 2600,
      "expired_wishes": 150,
      "total_frozen_points": 22500
    },
    "economy_stats": {
      "total_love_points_issued_today": 45000,
      "total_diamonds_spent_today": 1200,
      "total_group_exp_earned_today": 22000,
      "avg_love_point_per_order": 5.3
    },
    "risk_stats": {
      "pending_review_orders": 12,
      "suspected_fraud_orders": 3,
      "banned_users_today": 2
    },
    "sign_in_stats": {
      "total_sign_ins_today": 4800,
      "full_team_sign_ins_today": 1200,
      "avg_consecutive_days": 4.5
    }
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### 16.3 管理员趋势数据

**接口**: `GET /api/admin/dashboard/trends`

**认证**: 是（管理员）

**查询参数**:
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `metric` | string | 是 | 指标类型：DAU / ORDERS / WISHES / LOVE_POINTS / SIGN_INS |
| `start_date` | string | 是 | 开始日期 |
| `end_date` | string | 是 | 结束日期 |
| `granularity` | string | 否 | DAY / HOUR，默认 DAY |

**响应体**:

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "metric": "ORDERS",
    "granularity": "DAY",
    "data_points": [
      { "date": "2026-06-01", "value": 8200 },
      { "date": "2026-06-02", "value": 8400 },
      { "date": "2026-06-03", "value": 8500 }
    ]
  },
  "trace_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

## 附录：错误码完整表

| 错误码                              | 说明                                   |
| ----------------------------------- | -------------------------------------- |
| `AUTH_INVALID_TOKEN`                | 登录态无效或已过期                     |
| `AUTH_TOKEN_REVOKED`                | 令牌已被撤销                           |
| `AUTH_ACCOUNT_BANNED`               | 账号已被禁用                           |
| `AUTH_CODE_INVALID`                 | 微信 code 无效                         |
| `USER_NOT_FOUND`                    | 用户不存在                             |
| `USER_PHONE_ALREADY_BOUND`          | 手机号已被其他用户绑定                 |
| `USER_NICKNAME_INVALID`             | 昵称包含敏感词或超出长度               |
| `USER_ALREADY_IN_GROUP`             | 你已在其他小组                         |
| `USER_GROUP_NOT_EMPTY`              | 仍有关联的小组，请先退出               |
| `GROUP_NOT_FOUND`                   | 小组不存在                             |
| `GROUP_MEMBER_LIMIT_EXCEEDED`       | 小组成员超过 2 人                      |
| `GROUP_EXIT_SETTLEMENT_REQUIRED`    | 退出组前仍有未结清订单、心愿、冻结积分 |
| `INVITE_CODE_INVALID`               | 邀请码无效或已过期                     |
| `INVITE_CODE_USED`                  | 邀请码已使用                           |
| `INVITE_USER_MISMATCH`              | 当前用户与邀请用户不匹配               |
| `ROLE_SWAP_BLOCKED_BY_ORDER`        | 存在未完结订单，禁止互换               |
| `ROLE_SWAP_BLOCKED_BY_WISH`         | 操作人存在在途心愿，禁止互换           |
| `PERMISSION_DENIED`                 | 无权限                                 |
| `ROLE_NOT_ALLOWED`                  | 当前角色不允许操作                     |
| `FOOD_NOT_FOUND`                    | 菜品不存在                             |
| `FOOD_CAPACITY_EXCEEDED`            | 菜品数量已达上限                       |
| `ORDER_NOT_FOUND`                   | 订单不存在                             |
| `ORDER_STATUS_INVALID`              | 订单状态不允许当前操作                 |
| `ORDER_TYPE_INVALID`                | 订单类型不支持当前操作                 |
| `DAILY_REWARD_LIMIT_REACHED`        | 每日积分或经验上限已达                 |
| `WISH_NOT_FOUND`                    | 心愿不存在                             |
| `WISH_STATUS_INVALID`               | 心愿状态不允许当前操作                 |
| `WISH_NOT_YOURS`                    | 你不是该心愿的发起人或履约人           |
| `LOVE_POINT_INSUFFICIENT`           | 爱心积分不足                           |
| `AGREEMENT_NOT_MUTUAL`              | 需双方均确认后才能进入心愿池           |
| `QUALITY_ALREADY_REVIEWED`          | 该心愿已进行过质量评价                 |
| `WISH_NOT_FINISHED`                 | 心愿未完成，无法查看质量               |
| `DIAMOND_INSUFFICIENT`              | 组钻石不足                             |
| `ADMIN_DAILY_DIAMOND_LIMIT_REACHED` | 管理员今日钻石发放已达上限             |
| `SIGN_IN_ALREADY_DONE`              | 今天已经签到过了                       |
| `UPLOAD_SIZE_EXCEEDED`              | 文件大小超出限制（最大 5MB）           |
| `UPLOAD_TYPE_NOT_ALLOWED`           | 不支持的文件类型                       |
| `UPLOAD_CONTENT_REJECTED`           | 上传内容审核未通过                     |
| `IDEMPOTENCY_CONFLICT`              | 幂等键对应请求内容冲突                 |
| `INVALID_PARAMETER`                 | 参数校验失败                           |
| `INTERNAL_ERROR`                    | 服务器内部错误                         |

---

## 附录：WebSocket 消息类型汇总

| 方向 | type                    | 说明         |
| ---- | ----------------------- | ------------ |
| C→S  | `auth`                  | 鉴权帧       |
| S→C  | `auth_ok`               | 鉴权成功     |
| S→C  | `auth_failed`           | 鉴权失败     |
| C→S  | `ping`                  | 心跳         |
| S→C  | `pong`                  | 心跳响应     |
| C→S  | `subscribe`             | 订阅组消息   |
| S→C  | `subscribed`            | 订阅确认     |
| S→C  | `notification`          | 通知消息     |
| S→C  | `order_status_changed`  | 订单状态变更 |
| S→C  | `wish_status_changed`   | 心愿状态变更 |
| S→C  | `love_point_changed`    | 积分变动     |
| S→C  | `diamond_changed`       | 钻石变动     |
| S→C  | `wish_fulfill_reminder` | 履约提醒     |
| S→C  | `sign_in_reminder`      | 签到提醒     |

---

**文档版本**: 2026-06-03
**状态**: 待审核
