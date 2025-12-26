# 小游戏联机后端（WS + 房间码）对接说明

本文档给前端同学使用：包含「房间码接口」与「WebSocket /ws/game 协议」的后端实现约定。

> 服务端项目：may_store（Rust + ntex）

## 1. 房间码接口（HTTP）

### 1.1 获取房间码

- 方法：`GET`
- 路径：`/game/room-code`
- 说明：生成 6 位房间码（A-Z0-9），服务端会尽量避开当前内存中已存在的房间码。

响应示例：

```json
{ "roomCode": "AB12CD" }
```

建议前端流程：
- 点击“创建房间” -> 先调用 `/game/room-code` 获取 roomCode
- 然后建立 WS 连接，并发送 `join_room`，带 `createIfNotExists=true`

## 2. WebSocket 连接

- WS 路径：`/ws/game`
- 消息格式：文本帧 JSON
  - 客户端 -> 服务端：`{ "type": string, "payload": object }`
  - 服务端 -> 客户端：`{ "type": string, "payload": object }`

### 2.1 小程序真机/预览的 Origin

小程序真机/预览环境通常会携带：

- `Origin: https://servicewechat.com`

本服务端已在 CORS 里放行该 Origin；如果你自行改过 CORS 配置，确保不要把该 Origin 拦截掉，否则可能出现：

- `1004 open fail: invalid http status`

服务端机器上可用以下命令快速排查：

- 不带 Origin：`wscat -c ws://<host>:9831/ws/game`
- 带小程序 Origin：`wscat -H "Origin: https://servicewechat.com" -c ws://<host>:9831/ws/game`

后端对未知 `type` 会回：

```json
{ "type": "error", "payload": { "message": "..." } }
```

## 3. 房间状态（room_state）

服务端会在这些时机推送/广播 `room_state`：
- join_room 成功
- leave_room 成功
- set_ready 成功
- start_game 成功
- 连接断开触发离开房间

结构：

```json
{
  "type": "room_state",
  "payload": {
    "roomCode": "AB12CD",
    "hostUserId": 10001,
    "status": "lobby",
    "members": [
      {
        "userId": 10001,
        "nickName": "张三",
        "avatar": "https://...",
        "ready": true,
        "isHost": true,
        "joinedAt": 1730000000000
      }
    ],
    "game": { "key": "WITNESS_WEREWOLF", "startedAt": 1730000000000 }
  }
}
```

- `status`：`lobby | started`
- `members`：前端可直接渲染（至少依赖 `userId/nickName/ready`）

## 4. 客户端 -> 服务端消息

### 4.1 join_room

```json
{
  "type": "join_room",
  "payload": {
    "roomCode": "AB12CD",
    "userId": 10001,
    "nickName": "张三",
    "avatar": "https://...",
    "createIfNotExists": true
  }
}
```

后端行为：
- `createIfNotExists=true`：房间不存在则创建，加入者成为房主
- `createIfNotExists=false`：房间不存在会返回 `error`（message=房间不存在）
- 同 `userId` 重复 join：幂等更新 nickName/avatar，保留 ready

### 4.2 leave_room

```json
{ "type": "leave_room", "payload": { "roomCode": "AB12CD", "userId": 10001 } }
```

- 房主离开会自动转移给当前成员列表中的第一个
- 房间无人后会自动销毁

### 4.3 set_ready

```json
{ "type": "set_ready", "payload": { "roomCode": "AB12CD", "userId": 10002, "ready": true } }
```

- 仅 `status=lobby` 允许

### 4.4 start_game

```json
{ "type": "start_game", "payload": { "roomCode": "AB12CD", "userId": 10001, "gameKey": "WITNESS_WEREWOLF" } }
```

校验：
- 仅房主可开始
- 必须 `status=lobby`
- `members.length >= 4`
- 全员 ready

成功后：
- 广播 `room_state`（status -> started）
- 由后端直接发身份（role_reveal，定向）

### 4.5 broadcast

```json
{ "type": "broadcast", "payload": { "roomCode": "AB12CD", "userId": 10001, "text": "天黑请闭眼" } }
```

后端会做简单限频（避免刷屏）。

### 4.6 role_reveal（兜底转发）

如果前端仍保留“房主兜底发牌”逻辑，服务端也支持房主发送 role_reveal 并定向转发：

```json
{
  "type": "role_reveal",
  "payload": {
    "roomCode": "AB12CD",
    "userId": 10001,
    "toUserId": 10002,
    "role": "WITNESS",
    "wolfUserIds": [10003, 10005],
    "wolfCount": 2,
    "totalPlayers": 6
  }
}
```

限制：
- 仅 started 阶段
- 仅房主可发

## 5. 服务端 -> 客户端消息

### 5.1 broadcast

```json
{
  "type": "broadcast",
  "payload": {
    "text": "天黑请闭眼",
    "at": 1730000000000,
    "fromUserId": 10001
  }
}
```

### 5.2 role_reveal（定向）

```json
{
  "type": "role_reveal",
  "payload": {
    "toUserId": 10002,
    "role": "WITNESS",
    "wolfUserIds": [10003, 10005],
    "wolfCount": 2,
    "totalPlayers": 6
  }
}
```

前端按 `toUserId == 自己` 展示弹窗。

## 6. 目击狼人杀发牌规则（后端已实现）

- 角色：`WOLF | VILLAGER | WITNESS`
- 狼人数量：
  - 4~5：1 狼
  - 6~8：2 狼
  - 9~11：3 狼
  - 12~15：4 狼
  - >15：floor(n/3)，至少 1
- 目击者：人数 >= 5 时额外加入 1 名
- 信息差：
  - 狼人：下发同伴名单（wolfUserIds 不包含自己）
  - 目击者：下发全部狼人名单（wolfUserIds 包含全部狼人）
  - 平民：wolfUserIds 为空
