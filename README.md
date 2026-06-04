# may-store（后端）

Rust + ntex 的后端服务，包含用户/菜品/订单/心愿/看板接口，并提供两种小游戏联机能力：

- **Socket 模式**：自建 WebSocket（`/ws/game`）+ 房间码（`/game/room-code`）
- **IM 模式**：基于腾讯云 IM（TIM/Chat），后端签发 UserSig（`/im/usersig`）并提供房间/游戏 REST 接口

服务默认监听：`0.0.0.0:9831`

## API 文档

- Swagger UI：`http://localhost:9831/swagger-ui/`
- OpenAPI JSON：`http://localhost:9831/api-doc/openapi.json`

## 快速开始（本地）

### 1) 准备依赖

- Rust（edition 2021）
- PostgreSQL（建议 15+）
- Redis

### 2) 初始化数据库

项目自带 PostgreSQL schema：

- [src/v3.sql](src/v3.sql)

示例（按你的连接信息修改）：

```bash
psql "postgres://postgres:<password>@localhost:5432/store_v2" -f src/v3.sql
```

历史迁移脚本（已归档，仅作存档参考，不要在干净的 v3 数据库上执行）：

- [src/utils/migrations_legacy/](src/utils/migrations_legacy/)

### 3) 配置环境变量

服务会读取 `.env`（通过 dotenvy），同时也支持直接从系统环境变量读取。

**必填**：

- `DATABASE_URL`：PostgreSQL 连接串（例如 `postgres://postgres:<password>@localhost:5432/store_v2`）
- `REDIS_URL`：Redis 连接串（例如 `redis://127.0.0.1/`）

**可选**：

- `FRONTEND_ORIGIN`：前端 origin 白名单。未设置或设置为 `*` 时会放行所有 origin。
	- 小程序真机/预览通常会带 `Origin: https://servicewechat.com`，服务端也会额外放行该 origin。

**腾讯云 IM（可选，启用 IM 模式才需要）**：

- `TENCENT_IM_SDK_APP_ID`：必填（启用 IM 时），数字
- `TENCENT_IM_SECRET_KEY`：必填（启用 IM 时）
- `TENCENT_IM_EXPIRE_SECONDS`：可选，默认 `86400`
- `TENCENT_IM_ADMIN_IDENTIFIER`：可选，默认 `administrator`

> 未配置 IM 环境变量时，IM 相关接口会返回明确的 400 错误提示。

### 4) 启动

```bash
cargo run
```

发布模式：

```bash
cargo run --release
```

备注：服务启动时会额外启动一个“订单过期”后台任务（不阻塞主 HTTP 服务）。

## Docker

本项目提供了 [Dockerfile](Dockerfile)（Ubuntu 运行时镜像）。构建：

```bash
docker build -t may-store .
```

运行（请替换为你自己的连接串）：

```bash
docker run --rm -p 9831:9831 \
	-e DATABASE_URL="postgres://postgres:<password>@<host>:5432/store_v2" \
	-e REDIS_URL="redis://<host>:6379/" \
	-e FRONTEND_ORIGIN="*" \
	may-store
```

如需启用腾讯云 IM，再追加相关环境变量即可。

## 接口概览

认证方式：多数需要登录态的接口使用 `Authorization: Bearer <token>`。

常用入口（更完整内容以 Swagger 为准）：

- 用户：`POST /register`、`POST /login`、`GET/POST /users`、`POST /users/checkin`
- 菜品：`GET/POST /foods`、`GET /foods/{id}`、`PUT/DELETE /foods/{id}`、`GET /foods/marks`
- 订单：`GET/POST /orders`、`PUT /orders/status`、`GET /orders/{id}`
- 心愿：`GET/POST /wishes`、`POST /wish_claims`、`POST/GET /wish_claims/{claim_id}/checkins`
- 看板：`GET /dashboard/*`、`GET /groups/{group_id}/activities`
- 上传：`GET /upload-token`（七牛上传 token）

## 小游戏联机

### Socket 模式（WS）

- 房间码：`GET /game/room-code`
- WebSocket：`GET /ws/game`
- 协议说明：见 [docs/game-ws-backend.md](docs/game-ws-backend.md)

### IM 模式（腾讯云 IM）

- 获取当前用户 UserSig：`GET /im/usersig`
- 房间/游戏：`GET /game/rooms`、`POST /game/rooms/{group_id}/start`、`POST /game/rooms/{group_id}/vote`
- 对接说明：见 [docs/tencent-im-backend.md](docs/tencent-im-backend.md)

## 安全提醒（开发/上线前必看）

当前仓库中存在部分第三方服务 key/secret 的硬编码常量（见 [src/utils.rs](src/utils.rs)）。
建议在上线前改为使用环境变量/密钥管理，并避免将真实密钥提交到代码仓库。

