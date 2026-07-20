# 第一阶段：使用 Debian 系 Rust 工具链构建应用
FROM rust:bookworm AS builder

ENV CARGO_HOME=/usr/local/cargo
ENV SQLX_OFFLINE=true

WORKDIR /app

# 先拷贝 manifest 缓存依赖(利用 Docker 缓存)
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main(){}" > src/main.rs && \
    cargo build --release --locked && \
    rm -rf src

# 拷贝真实源码、编译期迁移和 SQLx 离线缓存后构建
COPY src ./src
COPY migrations ./migrations
COPY .sqlx ./.sqlx
RUN touch src/main.rs && cargo build --release --locked

# 第二阶段：精简运行时镜像
FROM debian:bookworm-slim

# 安装运行时依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# 创建非 root 用户
RUN groupadd -r maystore && useradd -r -g maystore -d /app -s /sbin/nologin maystore

WORKDIR /app

# 从 builder 阶段复制产物
COPY --from=builder /app/target/release/may-store /app/may-store

# 容器元数据(运行时通过 -e 注入,严禁在此处硬编码)
# 必填:DATABASE_URL / REDIS_URL / JWT_SECRET
# 选填:WX_APP_ID / WX_APP_SECRET / QINIU_* / TENCENT_IM_* / FRONTEND_ORIGIN

EXPOSE 9831 9832

# 健康检查:每 30s 调一次,5s 超时,连续 3 次失败视为不健康
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:9831/health || exit 1

# 切换到非 root 用户
USER maystore

# 启动入口
CMD ["/app/may-store"]
