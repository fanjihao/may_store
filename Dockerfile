# #使用最新的Rust官方镜像
# FROM rust:latest
# # 1. This tells docker to use the Rust official image

# RUN rustup target add x86_64-unknown-linux-musl

# # 2. Copy the files in your machine to the Docker image
# COPY ./ ./

# EXPOSE 9831
# # Build your program for release
# RUN cargo build --release

# # Run the binary
# CMD ["./target/release/may-store"]

## 查看镜像
# docker ps -a
## 传输文件
# docker cp 72be1c824770:/target/x86_64-unknown-linux-gnu/release/may-store /rust-app

# 第一阶段：使用Rust工具链构建Rust应用
FROM rust:latest as builder

# 设置CARGO_HOME环境变量，指定Rust的依赖项源
ENV CARGO_HOME=/usr/local/cargo
ENV DATABASE_URL=postgres://postgres:root@124.223.60.157:5432/store

WORKDIR /app

# 将Cargo.toml和Cargo.lock拷贝到容器中并下载依赖
COPY Cargo.toml Cargo.lock ./
# RUN cargo build --release

# 拷贝应用源代码并构建可执行文件
COPY src ./src
COPY static ./static
RUN cargo build --release

CMD ["/bin/sh"]

# 第二阶段：创建最终的Docker镜像
FROM ubuntu:latest

RUN sed -i 's/archive.ubuntu.com/mirrors.aliyun.com/g' /etc/apt/sources.list

# 安装所需的运行时依赖
# RUN apk add --no-cache libgcc
# RUN apk add --no-cache libc6-compat
# RUN apk add --no-cache musl
RUN apt-get update && apt-get install -y libssl-dev
# RUN apk add --no-cache openssl-dev
# RUN ln -s /usr/local/lib/libssl.so.3 /usr/lib/libssl.so.3
# RUN ln -s /usr/local/lib/libcrypto.so.3 /usr/lib/libcrypto.so.3
EXPOSE 9831

WORKDIR /app

# 从第一阶段中复制可执行文件
COPY --from=builder /app/target/release/may-store .
COPY --from=builder /app/static .
COPY --from=builder /app/Cargo.lock .
COPY --from=builder /app/Cargo.toml .

ENV DATABASE_URL=postgres://postgres:root@124.223.60.157:5432/store

# 设置启动命令
CMD ["./may-store"]

