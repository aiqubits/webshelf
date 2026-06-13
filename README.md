# WebShelf

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![Rust-Agent](https://img.shields.io/badge/webshelf-release-yellow)](https://crates.io/crates/webshelf)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/aiqubits/webshelf/pulls)
[![Live Demo](https://img.shields.io/badge/demo-live-success)](https://www.openpick.org/webshelf)

**The best way to develop your web service with one click.**

WebShelf 是一个生产就绪的 Rust 全栈框架，建立在 Axum 和 Dioxus 基础上，包含完整的后端脚手架、认证系统、数据库集成、分布式锁支持和全面的中间件。

## ✨ 特性

- 🔐 **JWT 认证** - 基于令牌的安全认证，Argon2 密码哈希
- 🗄️ **数据库集成** - PostgreSQL 支持 (SeaORM 异步 ORM)
- 🔒 **分布式锁** - Redis 分布式锁，可选配置
- 🛡️ **中间件栈** - Panic 捕获、CORS、追踪、认证层
- ✅ **输入验证** - 邮箱和密码规则验证
- 📝 **结构化日志** - 基于 Tracing 的日志，支持多日志级别
- ⚙️ **灵活配置** - TOML 配置文件 + CLI 参数覆盖支持
- 🧪 **测试框架** - 单元测试和集成测试支持
- 🚀 **RESTful API** - 完整的用户管理 CRUD 操作
- 📦 **生产就绪** - 完善的错误处理、压缩、优雅关闭
- 🌐 **全栈框架** - 后端 (Axum) + 前端 (Dioxus/WASM 多端: Web/Desktop/Mobile) + 反向代理 (Nginx)
- 🐳 **容器化** - Docker Compose 和 Kubernetes 支持
- 🔄 **灰度部署** - 支持滚动升级和金丝雀发布

## 📋 系统要求

- **Rust** 1.92 或更高版本
- **PostgreSQL** 16+ (推荐 16.0+)
- **Redis** 7+ (可选，用于分布式锁，推荐 7.0+)
- **Docker** 和 **Docker Compose** (可选，用于容器化部署)

## 🚀 快速开始

### 1. 克隆并设置

```bash
git clone https://github.com/aiqubits/webshelf.git
cd webshelf
```

### 2. 配置数据库

创建 Docker 网络：
```bash
docker network create webshelf-net
```

启动 PostgreSQL：
```bash
docker run --name webshelf-postgres \
  --network webshelf-net \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=password \
  -e POSTGRES_DB=webshelf \
  -p 5432:5432 \
  --restart unless-stopped \
  -d postgres:16-alpine
```

启动 Redis：
```bash
docker run --name webshelf-redis \
  --network webshelf-net \
  -p 6379:6379 \
  --restart unless-stopped \  
  -d redis:7-alpine
```

### 3. 配置应用

复制配置文件并编辑：
```bash
cp config.toml.example config.toml
# 编辑 config.toml，填写数据库和 Redis 连接信息
```

### 4. 运行服务器

```bash
cargo run --package webshelf-server
```

服务器将在 `http://0.0.0.0:3000` 启动。

### 5. Docker Compose 快速启动 (推荐)

```bash
# 创建 .env 文件
cp .env.example .env
# 编辑 .env，设置 WEBSHELF_JWT_SECRET 等

# 启动所有服务
docker compose up -d

# 查看日志
docker compose logs -f
```

## 🔧 配置说明

### 配置文件结构 (`config.toml`)

```toml
# 数据库连接 (PostgreSQL)
# 将 CHANGE_ME_POSTGRES_PASSWORD 替换为强密码
database_url = "postgres://postgres:CHANGE_ME_POSTGRES_PASSWORD@127.0.0.1:5432/webshelf"

# Redis 连接 (分布式锁)
# 将 CHANGE_ME_REDIS_PASSWORD 替换为强密码
redis_url = "redis://:CHANGE_ME_REDIS_PASSWORD@127.0.0.1:6379"

# JWT 设置
# ⚠️ 生产环境必须修改！使用以下命令生成强密钥：
#   openssl rand -base64 64
jwt_secret = "REPLACE_ME_WITH_A_STRONG_SECRET"
jwt_expiry_seconds = 3600

# 服务器配置
[server]
host = "0.0.0.0"
port = 3000
# CORS 允许的源列表（生产环境必须配置）
# ⚠️ 在非开发环境 (staging/production) 中，如果此列表为空，
#   服务器将记录错误并阻止所有跨域请求。
# allowed_origins = ["https://example.com", "https://app.example.com"]

# 数据库连接池配置
[database]
max_connections = 10
min_connections = 1
```

### 环境变量覆盖

所有配置选项都可通过环境变量覆盖，格式为 `WEBSHELF_<OPTION>`：

```bash
# 覆盖数据库 URL
export WEBSHELF_DATABASE_URL="postgres://..."

# 覆盖 Redis URL
export WEBSHELF_REDIS_URL="redis://..."

# 覆盖 JWT 密钥（非常重要！）
export WEBSHELF_JWT_SECRET="your-strong-secret"

# 覆盖服务器主机和端口
export WEBSHELF_SERVER__HOST="127.0.0.1"
export WEBSHELF_SERVER__PORT="8080"

# 设置日志级别 (trace, debug, info, warn, error)
export RUST_LOG="info"

# 设置运行环境 (development, staging, production)
export WEBSHELF_ENV="development"

# PostgreSQL 密码（Docker Compose 环境必需）
export WEBSHELF_POSTGRES_PASSWORD="CHANGE_ME_POSTGRES_PASSWORD"

# Redis 密码（Docker Compose 环境必需）
export WEBSHELF_REDIS_PASSWORD="CHANGE_ME_REDIS_PASSWORD"
```

**密钥生成指南:**

生成强随机密钥用于生产环境：

```bash
# 生成 JWT 密钥 (64 字符 Base64)
openssl rand -base64 64

# 生成数据库密码 (32 字符十六进制)
openssl rand -hex 32

# 生成 Redis 密码 (32 字符十六进制，避免 URL 特殊字符)
openssl rand -hex 32
```

### 命令行参数

```bash
cargo run --package webshelf-server -- [OPTIONS]

选项:
  -H, --host <HOST>              服务器绑定地址 [默认: 0.0.0.0]
  -P, --port <PORT>              服务器端口 [默认: 3000]
  -E, --env <ENV>                环境 [默认: development]
  -C, --config <CONFIG>          配置文件路径 [默认: config.toml]
  -L, --log-level <LOG_LEVEL>    日志级别 [默认: info]
  -h, --help                     显示帮助
  -V, --version                  显示版本
```

示例：
```bash
cargo run --package webshelf-server -- --host 127.0.0.1 --port 3000 --log-level debug
```

## 📚 API 文档

### 基础 URL
```
http://127.0.0.1:3000/api
```

### 健康检查

#### 健康状态
```http
GET /api/health
```

响应:
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

### 认证端点

#### 注册用户
```http
POST /api/public/auth/register
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "SecurePass123",
  "name": "User Name"
}
```

响应 (201 Created):
```json
{
  "message": "User registered successfully",
  "user_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**密码要求:**
- 最少 8 字符
- 至少包含 1 个小写字母
- 至少包含 1 个大写字母
- 至少包含 1 个数字

#### 登录
```http
POST /api/public/auth/login
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "SecurePass123"
}
```

响应 (200 OK):
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
  "token_type": "Bearer",
  "expires_in": 3600,
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "role": "user"
}
```

### 用户管理

#### 创建用户 (需要认证)
```http
POST /api/users
Authorization: Bearer <token>
Content-Type: application/json

{
  "email": "newuser@example.com",
  "password": "SecurePass123",
  "name": "New User"
}
```

响应 (201 Created):
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "email": "newuser@example.com",
  "name": "New User",
  "role": "user",
  "created_at": "2026-06-08T06:00:00Z",
  "updated_at": "2026-06-08T06:00:00Z"
}
```

#### 获取用户 (需要认证)
```http
GET /api/users/{id}
Authorization: Bearer <token>
```

响应 (200 OK):
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "email": "user@example.com",
  "name": "User Name",
  "role": "user",
  "created_at": "2026-01-11T06:00:00Z",
  "updated_at": "2026-01-11T06:00:00Z"
}
```

#### 更新用户 (需要认证)
```http
PUT /api/users/{id}
Authorization: Bearer <token>
Content-Type: application/json

{
  "email": "updated@example.com",
  "name": "Updated Name",
  "role": "user"
}
```

响应 (200 OK):
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "email": "updated@example.com",
  "name": "Updated Name",
  "role": "user",
  "created_at": "2026-01-11T06:00:00Z",
  "updated_at": "2026-06-08T10:30:00Z"
}
```

#### 删除用户 (需要认证)
```http
DELETE /api/users/{id}
Authorization: Bearer <token>
```

响应 (204 No Content)

#### 列表用户 - 分页 (需要认证)
```http
GET /api/users?page=1&per_page=10
Authorization: Bearer <token>
```

响应 (200 OK):
```json
{
  "items": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "user1@example.com",
      "name": "User One",
      "role": "user",
      "created_at": "2026-01-11T06:00:00Z",
      "updated_at": "2026-01-11T06:00:00Z"
    }
  ],
  "total": 42,
  "page": 1,
  "per_page": 10,
  "total_pages": 5
}
```

### 错误处理

所有错误响应都遵循统一的格式：

```json
{
  "error": "error_type",
  "message": "Detailed error message"
}
```

**错误类型:**
- `bad_request` (400) - 请求参数错误
- `unauthorized` (401) - 缺少或无效的认证
- `forbidden` (403) - 权限不足
- `not_found` (404) - 资源不存在
- `conflict` (409) - 资源冲突 (如邮箱重复)
- `validation_error` (400) - 输入验证失败
- `internal_error` (500) - 服务器内部错误
- `service_unavailable` (503) - 服务不可用

## 🏗️ 项目结构

```
webshelf/
├── server/                      # 后端服务 (Rust + Axum)
│   ├── migrations/              # 数据库迁移脚本
│   │   └── 001_init.sql
│   ├── src/
│   │   ├── handlers/            # HTTP 请求处理程序
│   │   │   ├── api.rs           # API 端点处理 (CRUD)
│   │   │   ├── auth.rs          # 认证端点处理 (登录/注册)
│   │   │   └── mod.rs
│   │   ├── middlewares/         # HTTP 中间件
│   │   │   ├── auth.rs          # JWT 认证中间件
│   │   │   ├── panic.rs         # Panic 捕获中间件
│   │   │   └── mod.rs
│   │   ├── repositories/        # 数据访问层 (DAL)
│   │   │   ├── user.rs          # 用户 Entity 和查询
│   │   │   └── mod.rs
│   │   ├── routes/              # 路由定义
│   │   │   ├── api.rs           # API 路由
│   │   │   ├── auth.rs          # 认证路由
│   │   │   └── mod.rs
│   │   ├── services/            # 业务逻辑层
│   │   │   ├── auth.rs          # 认证服务
│   │   │   ├── user.rs          # 用户服务
│   │   │   ├── lock.rs          # 分布式锁服务
│   │   │   └── mod.rs
│   │   ├── utils/               # 工具模块
│   │   │   ├── config.rs        # 配置加载
│   │   │   ├── error.rs         # 错误类型定义
│   │   │   ├── logger.rs        # 日志初始化
│   │   │   ├── password.rs      # 密码哈希和验证
│   │   │   ├── validator.rs     # 输入验证
│   │   │   └── mod.rs
│   │   ├── bootstrap.rs         # 应用启动和初始化
│   │   ├── lib.rs               # 库导出
│   │   ├── main.rs              # 应用入口
│   │   └── migrations.rs        # 数据库迁移运行器
│   ├── tests/
│   │   └── integration_tests.rs # 集成测试
│   └── Cargo.toml               # 服务器依赖
│
├── app/                          # 前端应用 (Dioxus 多端)
│   ├── ui/                      # UI 组件库
│   │   ├── src/
│   │   │   ├── hero.rs          # Hero 组件
│   │   │   ├── navbar.rs        # 导航栏组件
│   │   │   └── lib.rs
│   │   ├── assets/
│   │   │   └── styling/         # 样式文件
│   │   └── Cargo.toml
│   ├── web/                     # Web 前端应用 (Dioxus/WASM)
│   │   ├── src/
│   │   │   ├── main.rs          # 前端入口
│   │   │   └── views/           # 页面视图
│   │   ├── assets/              # 静态资源
│   │   └── Cargo.toml
│   ├── desktop/                 # 桌面应用 (Dioxus Desktop)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   └── views/
│   │   ├── assets/
│   │   └── Cargo.toml
│   └── mobile/                  # 移动应用 (Dioxus Mobile)
│       ├── src/
│       │   ├── main.rs
│       │   └── views/
│       ├── assets/
│       └── Cargo.toml
│
├── nginx/
│   └── default.conf             # Nginx 反向代理配置
│
├── k8s/                         # Kubernetes 部署清单
│   ├── namespace.yaml           # 命名空间
│   ├── postgres.yaml            # PostgreSQL 部署
│   ├── redis.yaml               # Redis 部署
│   ├── webshelf.yaml            # 应用主部署
│   ├── webshelf-web.yaml        # 前端部署 (可选)
│   ├── configmap.yaml           # 配置映射
│   ├── secret.yaml.example      # 密钥示例
│   └── ingress.yaml             # Ingress 配置
│
├── .github/
│   └── workflows/
│       └── webshelf.yaml        # CI/CD 流程
│
├── docker-compose.yml           # Docker Compose 编排文件
├── Dockerfile.server            # 服务器容器镜像
├── Dockerfile.web               # 前端容器镜像
├── config.toml.example          # 配置示例
├── Cargo.toml                   # 工作区根配置
├── README.md                    # 本文件
├── DEPLOYMENT.md                # 部署指南
└── LICENSE                      # MIT 许可证
```

## 🏛️ 架构概览

### 三层架构

```
┌─────────────────────────────────────────┐
│     Frontend (app/)                        │  多端应用 (Web/Desktop/Mobile)
├─────────────────────────────────────────┤
│     Nginx (反向代理)                     │  路由/速率限制/安全头
├─────────────────────────────────────────┤
│  Backend (Axum) - 处理层                 │  HTTP 处理和路由
│  ├─ Middleware Stack                   │
│  │  ├─ Panic Capture                  │
│  │  ├─ Authentication                 │
│  │  ├─ Tracing/Logging                │
│  │  └─ CORS                           │
│  ├─ Handlers/Routes                   │
│  └─ Services (业务逻辑)                 │
├─────────────────────────────────────────┤
│  Service Layer - 业务逻辑                │
│  ├─ UserService                       │
│  ├─ AuthService                       │
│  └─ LockService                       │
├─────────────────────────────────────────┤
│  Repository Layer - 数据访问              │
│  └─ UserRepository (SeaORM)            │
├─────────────────────────────────────────┤
│  Persistent Layer                      │
│  ├─ PostgreSQL (主数据存储)               │
│  └─ Redis (分布式锁)                    │
└─────────────────────────────────────────┘
```

### 中间件执行顺序

中间件从内到外执行：

1. **Panic 捕获** - 捕获 panic 并返回 500 错误
2. **认证** - JWT 令牌验证（受保护路由）
3. **Tracing** - 请求/响应日志
4. **CORS** - 跨域资源共享
5. **压缩** - 响应压缩 (Gzip/Brotli)

### 速率限制

Nginx 在反向代理层实现速率限制：

- **认证端点** (`/api/public/auth/`): 5 请求/分钟 (防止暴力破解)
- **通用 API** (`/api/`): 60 请求/分钟 (防止资源耗尽)

### 不同环境中的配置

根据部署方式的不同，Redis 和 PostgreSQL 的连接地址需要进行对应配置：

**本地开发环境:**
```bash
# 直接连接本地运行的 PostgreSQL 和 Redis
database_url = "postgres://postgres:CHANGE_ME_POSTGRES_PASSWORD@127.0.0.1:5432/webshelf"
redis_url = "redis://:CHANGE_ME_REDIS_PASSWORD@127.0.0.1:6379"
```

**Docker Compose 环境:**
```bash
# 使用 Docker 事件网络中的服务名称
# .env 文件中配置（注意 Redis 需要使用密码）：
WEBSHELF_DATABASE_URL=postgres://postgres:${WEBSHELF_POSTGRES_PASSWORD}@postgres:5432/webshelf
WEBSHELF_REDIS_URL=redis://:${WEBSHELF_REDIS_PASSWORD}@redis:6379
```

**Kubernetes 环境:**
```bash
# 使用 Kubernetes Service DNS 名称
# k8s/secret.yaml.example 中配置：
# Service 名称规约: postgres-service, redis-service, webshelf-service
WEBSHELF_DATABASE_URL=postgres://postgres:${WEBSHELF_POSTGRES_PASSWORD}@postgres-service.webshelf.svc.cluster.local:5432/webshelf
WEBSHELF_REDIS_URL=redis://:${WEBSHELF_REDIS_PASSWORD}@redis-service.webshelf.svc.cluster.local:6379
```

## 🧪 测试

### 运行单元测试

```bash
cargo test --package webshelf-server
```

### 运行集成测试

```bash
# 确保 PostgreSQL 和 Redis 已启动
cargo test --test integration_tests -- --test-threads=1
```

### 测试覆盖范围

- ✅ 密码哈希和验证
- ✅ 输入验证 (邮箱、密码)
- ✅ 配置加载和覆盖
- ✅ API 端点集成测试
- ✅ 用户 CRUD 操作

## 🔐 安全特性

### 密码安全

- **算法**: Argon2id (KDF)
- **盐化**: 自动生成唯一盐
- **哈希**: 不存储明文密码

### JWT 令牌

- **签名算法**: HS256
- **过期时间**: 可配置 (默认 1 小时)
- **强制刷新**: 过期后需重新登录

### 输入验证

- **邮箱验证**: RFC 5322 格式检查
- **密码强度**: 最少 8 字符，必含大小写字母和数字
- **长度限制**: 名字 2-50 字符

### HTTP 安全头

所有响应都包含以下安全头：

```
Strict-Transport-Security: max-age=31536000; includeSubDomains
X-Frame-Options: SAMEORIGIN
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; ...
```

### 可靠性特性

- **Panic 恢复**: 自动捕获 panic，返回 500 错误而不是崩溃
- **优雅关闭**: SIGTERM/SIGINT 信号处理
- **连接池**: PostgreSQL 连接池管理
- **健康检查**: 就绪状态检查端点

## 📦 依赖版本

### 核心依赖

- **axum** 0.8.8 - 异步网络框架
- **tokio** 1.x - 异步运行时
- **sea-orm** 1.x - 异步 ORM
- **redis** 0.27 - Redis 客户端
- **dioxus** 0.7.9 - 前端框架

### 认证和安全

- **jsonwebtoken** 9.x - JWT 处理
- **argon2** 0.5.x - 密码哈希
- **validator** 0.19.x - 输入验证

### 序列化和工具

- **serde** 1.x - 序列化/反序列化
- **serde_json** 1.x - JSON 处理
- **chrono** 0.4.x - 日期时间
- **uuid** 1.x - UUID 生成

### 日志和错误处理

- **tracing** 0.1.x - 结构化日志
- **tracing-subscriber** 0.3.x - 日志订阅
- **thiserror** 2.x - 自定义错误类型
- **anyhow** 1.x - 通用错误处理

## 🛠️ 开发指南

### 开发模式运行

```bash
# 完整日志和调试信息
cargo run --package webshelf-server -- \
  --env development \
  --log-level debug \
  --config config.toml
```

### 生产构建

```bash
# 优化的发布构建
cargo build --release --package webshelf-server
```

### 生产运行

```bash
./target/release/webshelf-server \
  --env production \
  --config prod.config.toml \
  --log-level warn
```

### 添加新的 API 端点

1. 在 `handlers/` 中创建处理程序函数
2. 在 `routes/` 中定义路由
3. 在 `bootstrap.rs` 中注册路由到路由器
4. 编写测试

### 添加新的数据库表

1. 在 `server/migrations/` 中创建 SQL 迁移文件
2. 命名格式: `NNN_description.sql` (例如 `002_create_posts_table.sql`)
3. 运行 `cargo run` 以自动执行新迁移
4. 在 `repositories/` 中定义相应的 Entity

## 📊 性能特性

- **异步 I/O**: 全异步编程，高并发能力
- **连接复用**: HTTP/1.1 Keep-Alive 和 Nginx 上游连接复用
- **响应压缩**: Gzip 和 Brotli 支持
- **静态资源缓存**: 7 天浏览器缓存
- **请求体限制**: 10MB 最大请求体 (防止 DoS)

## 🤝 贡献指南

欢迎贡献！请遵循以下步骤：

### 环境准备

确保已安装 [pre-commit](https://pre-commit.com)：

```bash
# 使用 pip 安装（推荐）
pip install pre-commit

# 或使用 pipx
pipx install pre-commit

# macOS
brew install pre-commit

# Ubuntu/Debian
sudo apt install pre-commit
```

### 安装 Git Hooks

```bash
# 在项目根目录执行
pre-commit install
```

安装后，每次 `git commit` 会自动按顺序执行以下检查：
1. **测试** — 运行 server/web/client-api 的测试
2. **Clippy** — 对所有 crate 进行静态分析
3. **Fmt** — 检查所有 crate 的代码格式

任何步骤失败则 commit 被终止，请根据提示修复后重新提交。

### 提交步骤

1. Fork 仓库
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 开启 Pull Request

## 📄 许可证

本项目采用 MIT 许可证 - 详见 [LICENSE](LICENSE) 文件。

## 👥 作者

- **aiqubits** - [aiqubits@hotmail.com](mailto:aiqubits@hotmail.com)
