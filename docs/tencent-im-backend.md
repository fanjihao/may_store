# 腾讯云 IM（后台）接入说明

本文档说明 may-store 后端如何接入腾讯云 IM（Chat/TIM）的 **UserSig** 能力，并提供后端接口给前端/管理后台联调用。

## 1. 背景与术语

- **SDKAppID**：腾讯云 IM 应用 ID。
- **identifier**：IM 登录的用户唯一标识（字符串）。本项目默认使用 `user_id` 的字符串形式。
- **UserSig**：服务端生成的签名，前端使用它登录腾讯云 IM SDK。

> 注意：UserSig 必须在服务端生成，`secret_key` 不能下发到前端。

## 2. 环境变量

后端通过环境变量读取 IM 配置：

- `TENCENT_IM_SDK_APP_ID`：必填，数字。
- `TENCENT_IM_SECRET_KEY`：必填，字符串。
- `TENCENT_IM_EXPIRE_SECONDS`：可选，默认 `86400`（24h）。

如果未配置，IM 接口会返回 400，并提示缺少配置。

## 3. 接口

### 3.1 获取当前用户 UserSig

- 方法：`GET`
- 路径：`/im/usersig`
- 鉴权：需要 `Authorization: Bearer <token>`（与现有用户体系一致）

响应示例：

```json
{
  "sdkAppId": 1400000000,
  "identifier": "12345",
  "userId": 12345,
  "userSig": "eJyrVkrLz1eyUkpKLFKqBQA...",
  "expireAt": 1760000000
}
```

字段说明：

- `identifier`：IM 登录用的用户标识（字符串）。
- `userSig`：给前端 IM SDK 使用的签名。
- `expireAt`：签名过期时间（Unix 秒）。

## 4. 前端/管理后台联调建议

1. 先登录后端，拿到后端颁发的 `token`。
2. 调用 `GET /im/usersig` 获取 `{ sdkAppId, identifier, userSig }`。
3. 用腾讯云 IM SDK 调用登录：
   - `SDKAppID = sdkAppId`
   - `userID = identifier`
   - `userSig = userSig`

## 5. 安全注意事项

- `TENCENT_IM_SECRET_KEY` 只允许出现在服务端环境变量中。
- 建议为 `TENCENT_IM_EXPIRE_SECONDS` 设置合理过期时间（例如 1h~24h）。
- 本项目当前接口仅为“当前登录用户”签发 UserSig；如需后台为任意用户签发，请在接口层加入管理员权限校验。
