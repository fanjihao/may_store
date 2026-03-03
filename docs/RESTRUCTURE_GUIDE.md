# may_store 项目实用架构重构指引 (Pragmatic Architecture)

本指引旨在为 `may_store` 项目提供一套**简单易读、精简干练且具备生产级稳定性与安全性**的后端架构方案。
针对初学者和中小型项目，我们放弃过度复杂的领域驱动设计（DDD）和抽象接口，采用“**按业务分包（模块化） + 极简三层**”的实用架构。

---

## 1. 核心设计原则

- **简单直白 (KISS)**：拒绝过度抽象。不需要为了解耦而写大量的 `Trait`（接口）和依赖注入。业务逻辑直接调用数据库连接执行 SQL。
- **高内聚，低耦合 (按业务分包)**：将相关联的代码放在同一个业务目录下（如 `users`, `orders`），而不是按技术栈分层（不要把所有模块的路由全塞进一个 `api` 目录）。
- **绝对防御 (防御性编程)**：
  - 业务代码中**绝对禁止使用 `unwrap()` 或 `expect()`**，所有错误必须通过 `?` 向上抛出。
  - 所有外部输入必须经过校验（类型校验、长度校验等）。
- **统一错误处理**：通过定义全局统一的错误枚举，自动将业务错误转换为规范的 HTTP JSON 响应（包含业务状态码和提示信息）。

---

## 2. 目标目录树 (Directory Map)

整个项目围绕“业务模块”展开，外层提供基础设施，内层处理具体业务。

```text
src/
├── main.rs              # 唯一入口：加载配置、初始化日志、启动 HTTP 服务
├── config.rs            # 全局配置管理 (数据库连接池、环境变量读取)
├── errors.rs            # 全局统一的错误处理 (AppError 枚举及 HTTP 响应映射)
├── middlewares/         # 拦截器 (中间件)
│   ├── auth.rs          # JWT 鉴权拦截器
│   └── logger.rs        # 请求日志拦截器
├── utils/               # 通用工具包
│   ├── crypto.rs        # 密码 Hash、加密解密
│   └── response.rs      # 统一的 JSON 返回体封装 (Success/Fail)
│
├── users/               # 【业务模块：用户】(包含注册、登录、信息修改等)
│   ├── mod.rs           # 模块导出声明
│   ├── routes.rs        # API 路由与控制器 (接收请求，参数校验，调用 service，返回结果)
│   ├── service.rs       # 核心业务与数据库操作 (直接写入 sqlx 查询，处理业务规则)
│   └── models.rs        # 数据结构 (数据库表结构 Entity，以及请求/响应的 DTO 结构)
│
├── orders/              # 【业务模块：订单】(包含创建订单、支付状态流转等)
│   ├── mod.rs
│   ├── routes.rs
│   ├── service.rs
│   └── models.rs
│
├── wishes/              # 【业务模块：心愿单】(包含许愿、打卡等)
│   ├── mod.rs
│   ├── routes.rs
│   ├── service.rs
│   └── models.rs
│
├── foods/               # 【业务模块：菜谱/食物】
│   ├── mod.rs
│   ├── routes.rs
│   ├── service.rs
│   └── models.rs
│
├── game_im/             # 【业务模块：即时通讯游戏】
│   ├── mod.rs
│   ├── routes.rs
│   ├── service.rs
│   └── models.rs
│
├── game_ws/             # 【业务模块：WebSocket游戏】
│   ├── mod.rs
│   ├── routes.rs
│   ├── service.rs
│   └── models.rs
│
├── wx/                  # 【业务模块：微信相关集成】(小程序登录、支付等)
│   ├── mod.rs
│   ├── routes.rs
│   ├── service.rs
│   └── models.rs
│
├── dashboard/           # 【业务模块：后台管理面板数据】
│   ├── mod.rs
│   ├── routes.rs
│   ├── service.rs
│   └── models.rs
│
└── upload/              # 【业务模块：文件上传】(如七牛云对接等)
    ├── mod.rs
    ├── routes.rs
    ├── service.rs
    └── models.rs
```

---

## 3. 分层职责拆解 (以 users 模块为例)

### 第一层：`routes.rs` (接口层/防线)
- **职责**：定义路由路径（如 `/api/v1/users/login`），接收 HTTP 请求，提取参数。
- **安全拦截**：使用 `validator` 等库校验参数合法性（如邮箱格式、密码长度）。若校验失败，直接返回 400 错误，**绝不让脏数据进入下一层**。
- **调用流转**：调用 `service.rs` 中的函数，拿到结果后封装为统一的 JSON 格式返回给前端。

### 第二层：`service.rs` (业务逻辑层/数据库层)
- **职责**：实现真正的业务规则。
- **数据库操作**：直接使用 `sqlx` 宏（如 `sqlx::query_as!`）对数据库进行增删改查。
- **事务管理**：涉及多表更新的操作（如扣减库存+生成订单），必须在这里开启并提交数据库事务 (`Transaction`)。

### 第三层：`models.rs` (数据模型层)
- **职责**：定义各种 `struct`。
- 包括与数据库表对应的结构体（带有 `#[derive(sqlx::FromRow)]`），以及接收前端请求的结构体（带有 `#[derive(Deserialize)]`）和返回给前端的结构体（带有 `#[derive(Serialize)]`）。

---

## 4. 上线与安全加固指南 (Production & Security Checklist)

在重构过程中，我们必须为系统建立以下防御机制：

### 4.1 稳定性保障
- [ ] **消灭 Panic**：全局搜索 `.unwrap()` 和 `.expect()`。除了在 `main.rs` 启动时的配置加载允许 panic 外，业务请求处理中必须替换为返回 `Result<T, AppError>`。
- [ ] **统一错误响应**：所有 API 报错时，前端收到的格式必须是标准化的（例如 `{"code": 400, "message": "密码错误"}`），绝不能把数据库原始报错信息（如 SQL 语法错误）暴露给外部。
- [ ] **全链路日志**：引入 `tracing`。每个核心操作都要打印 `info!` 或 `error!` 日志，方便线上排查。

### 4.2 安全防御
- [ ] **防 SQL 注入**：严格使用 `sqlx::query!` 的参数绑定机制（如 `$1`, `$2`），**禁止任何形式的 SQL 字符串拼接**。
- [ ] **越权访问防御 (BOLA)**：在修改或删除数据的 SQL 语句中，必须强制带上当前登录用户的 ID。例如：`UPDATE orders SET status = 'paid' WHERE id = $1 AND user_id = $2`。
- [ ] **密码安全**：数据库绝不存储明文密码。必须使用 `argon2` 或 `bcrypt` 进行单向哈希加盐存储。
- [ ] **接口限流 (Rate Limiting)**：关键接口（如登录、注册、发送验证码）必须在 Nginx 层或 Rust 应用层加限流，防止暴力破解和短信被刷。

---

## 5. 循序渐进的重构步骤

不要试图一次性改写所有代码。请按照以下顺序逐步重构：

1. **基础设施建设**：
   - 编写全局的 `errors.rs`，定义一套清晰的错误枚举。
   - 编写统一的 API 返回格式工具 (`utils/response.rs`)。
2. **重构首个模块 (打样)**：
   - 选择一个相对简单的模块（例如 `users`）。
   - 将其原有的代码拆分到 `users/routes.rs`, `users/service.rs`, `users/models.rs` 中。
   - 跑通接口，验证这一套极简架构的流畅度。
3. **加固安全防线**：
   - 引入 JWT 鉴权中间件。
   - 引入请求参数校验机制。
4. **全面迁移**：
   - 照猫画虎，把 `orders`, `wishes` 等其他模块逐一迁移到新架构。
   - 彻底清理遗留的旧代码。