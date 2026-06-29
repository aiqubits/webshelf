[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/aiqubits/webshelf/pulls)
[![Live Demo](https://img.shields.io/badge/demo-live-success)](https://www.openpick.org/webshelf)

# WebShelf

**The best way to develop your web service with one click.**

[English](README.md) | [简体中文](README.zh-CN.md)

[特性](#特性) • [快速开始](#快速开始) • [文档](#文档) • [许可证](#许可证)

## 概述

WebShelf 是一个全端生产就绪的系统框架，建立在 **纯 Rust 技术栈** 之上——后端 Axum/Salvo 等多运行时、前端 Dioxus 多端（Web/Desktop/Mobile）、SeaORM 异步 ORM、Redis 分布式三件套（缓存/限流/锁）。从代码到部署，全套事实标准，AI Coding 友好。

## 特性

### 全端 Rust 技术栈
- **后端多引擎** — 通过 `webshelf-runtime` 抽象层，同一套业务代码支持 [Axum](https://github.com/tokio-rs/axum) 和 [Salvo](https://github.com/salvo-rs/salvo) 运行时切换（feature flag），零代码改动
- **前端多端** — [Dioxus](https://dioxuslabs.com/) 0.7，一套 UI 组件库（`app/ui`）驱动 Web/Desktop/Mobile 三端
- **客户端 SDK** — `client-api` crate 封装认证、请求、类型，前后端共享
- **标准化嵌入，不过度封装** — Handler 直接使用框架原生类型，`Runtime` trait 仅抽象路由构建和服务启动，不遮蔽框架能力

### 主从读写分离数据库
- **`AutoRouter`** — 实现 SeaORM `ConnectionTrait` + `TransactionTrait`，INSERT/UPDATE/DELETE 自动走写库，SELECT 自动路由到读库
- **多策略负载均衡** — round_robin / random / weighted 三种读库选择策略
- **熔断器 + 健康检查** — 失败自动熔断，后台定时探测恢复，降级回写库
- **K8s 一键部署** — CloudNativePG 1 主 2 从集群，自动故障转移，`-rw`/`-ro` Service 端点
- **Docker Compose 双模式** — [`docker-compose.yml`](docker-compose.yml) 单实例 PostgreSQL；[`docker-compose.replicas.yml`](docker-compose.replicas.yml) 1 主 2 从流式复制集群，启用读写分离

### Redis 数据接口缓存 + 分布式限流 + 分布式锁
- **统一 `CacheService`** — bb8 连接池，`get_or_insert` 自动回填，`get_or_insert_with_lock` 缓存击穿保护（分布式锁防 stampede）
- **优雅降级** — Redis 不可用时所有操作静默 no-op，服务不崩溃
- **`distributed-ratelimit`** — 固定窗口限流，IP 级别 + 邮箱级别双重策略，精确到每个认证端点
- **`LockGuard`** — Lua 脚本原子释放，`Drop` 自动解锁，fail-open / fail-close 双策略

### Web 框架运行时切换
- **`Runtime` trait** — 定义 `Router` / `MethodRouter` / `with_route` / `nest` / `merge` / `with_state` / `serve` 七大操作
- **`webshelf-axum`** / **`webshelf-salvo`** 适配器 — 各自实现 `Runtime`，通过 Cargo feature `default = ["webshelf-axum"]` 切换
- **统一 Handler 签名** — `async fn(UnifiedRequest) -> Result<Response, HttpError>`，框架无关

### 安全体系
- **Argon2id** 密码哈希 — 自动盐化，KDF 标准
- **JWT** — HS256 签名，token_version 版本控制，支持记住我（30 天），Refresh Token 轮转
- **输入验证** — RFC 5322 邮箱，密码强度（大小写 + 数字 + 8 位），长度限制
- **HTTP 安全头** — HSTS / X-Frame-Options / X-Content-Type-Options / CSP

### 分布式基础设施
- **Snowflake ID** — Twitter 算法，DB 自动协调 worker_id，无锁原子生成，JSON 序列化为字符串（JS 精度安全）
- **国际化 i18n** — `crates/i18n`，过程宏自动翻译字段
- **微信集成** — `wechat-api` crate，公众号验证码登录

### AI Coding 友好
- **全套事实标准** — SeaORM、Axum/Salvo、Tracing、Serde、Chrono 等社区标准库，无自研框架
- **最少封装** — `Runtime` trait 每个方法对应一个操作，不隐藏框架细节
- **完善的文档和测试** — 单元 + 集成测试，完整 API 文档

---

## 快速开始

```bash
# 1. 克隆并进入项目
git clone https://github.com/aiqubits/webshelf.git
cd webshelf

# 2. 启动 PostgreSQL 和 Redis（Docker）
docker network create webshelf-net 2>/dev/null || true
docker run -d --network webshelf-net --name webshelf-postgres-dev \
  -e POSTGRES_PASSWORD=devpassword -e POSTGRES_DB=webshelf \
  -p 5432:5432 postgres:16-alpine
docker run -d --network webshelf-net --name webshelf-redis-dev \
  -p 6379:6379 redis:7-alpine

# 3. 配置
cp config.toml.example config.toml

# 4. 启动（默认 Axum 运行时）
cargo run -p webshelf-server -- --env development --log-level debug

# 5. 切换 Salvo 运行时
cargo run -p webshelf-server --features webshelf-salvo -- --env development --log-level debug
```

---

## 文档

| 文档 | 说明 |
|------|------|
| [架构概览](docs/architecture.md) | 运行时抽象、读写分离、Redis 三件套、部署架构、API 文档 |
| [部署指南](docs/deployment.md) | 本地开发、Docker Compose、K8s（含 CNP 集群）、配置管理 |
| [开发指南](docs/development.md) | 扩展端点、数据库迁移、运行时切换、贡献指南 |

---

## 许可证

MIT License - 详见 [LICENSE](LICENSE)。

## 作者

- **aiqubits** - [aiqubits@hotmail.com](mailto:aiqubits@hotmail.com)