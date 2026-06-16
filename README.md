# WebShelf

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![Rust-Agent](https://img.shields.io/badge/webshelf-release-yellow)](https://crates.io/crates/webshelf)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/aiqubits/webshelf/pulls)
[![Live Demo](https://img.shields.io/badge/demo-live-success)](https://www.openpick.org/webshelf)

**The best way to develop your web service with one click.**

WebShelf 是一个生产就绪的 Rust 全栈框架，建立在 Axum 和 Dioxus 基础上，包含完整的后端脚手架、认证系统、数据库集成、分布式锁和分布式限流支持、以及全面的中间件。

## ✨ 特性

- 🔐 **JWT 认证** - 基于令牌的安全认证，Argon2 密码哈希
- 🗄️ **数据库集成** - PostgreSQL 支持 (SeaORM 异步 ORM)
- 🔒 **分布式锁** - Redis 分布式锁
- 🚦 **分布式限流** - Redis 分布式速率限制
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
- **Redis** 7+ (用于分布式锁与分布式限流等，推荐 7.0+)
- **Docker** 和 **Docker Compose** (可选，用于容器化部署)

## 🚀 快速开始

完整的部署指南（包括本地开发、Docker Compose 和 Kubernetes 部署）请参阅 [DEPLOYMENT.md](DEPLOYMENT.md)。

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
│   │   │   ├── ratelimit.rs     # 分布式限流中间件
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
│   │   │   ├── password_reset.rs # 密码重置服务
│   │   │   ├── verification.rs  # 邮箱验证服务
│   │   │   └── mod.rs
│   │   ├── utils/               # 工具模块
│   │   │   ├── config.rs        # 配置加载
│   │   │   ├── error.rs         # 错误类型定义
│   │   │   ├── extractor.rs     # 自定义 Axum 提取器
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
│   │   │   ├── sidebar.rs       # 侧边栏组件
│   │   │   ├── top_header.rs    # 顶部导航组件
│   │   │   ├── app_shell.rs     # 应用外壳布局
│   │   │   ├── auth_form.rs     # 认证表单组件
│   │   │   ├── button.rs        # 按钮组件
│   │   │   ├── badge.rs         # 徽章组件
│   │   │   ├── modal.rs         # 模态对话框组件
│   │   │   ├── toast.rs         # 消息提示组件
│   │   │   ├── text_input.rs    # 文本输入组件
│   │   │   ├── data_table.rs    # 数据表格组件
│   │   │   ├── stats_card.rs    # 统计卡片组件
│   │   │   ├── route_card.rs    # 路由卡片组件
│   │   │   ├── code_console.rs  # 代码控制台组件
│   │   │   ├── global_styles.rs # 全局样式定义
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
├── crates/
│   ├── distributed-ratelimit/  # Redis 分布式限流
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── config.rs
│   │   │   ├── error.rs
│   │   │   └── limiter.rs
│   │   └── Cargo.toml
│   └── emailserver/           # SMTP 邮件发送
│       ├── src/
│       │   └── lib.rs
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
│  │  ├─ Rate Limiting (Redis)          │
│  │  ├─ Tracing/Logging                │
│  │  └─ CORS                           │
│  ├─ Handlers/Routes                   │
│  └─ Services (业务逻辑)                 │
├─────────────────────────────────────────┤
│  Service Layer - 业务逻辑                │
│  ├─ UserService                       │
│  ├─ AuthService                       │
│  ├─ LockService                       │
│  ├─ PasswordResetService              │
│  └─ VerificationService               │
├─────────────────────────────────────────┤
│  Repository Layer - 数据访问              │
│  └─ UserRepository (SeaORM)            │
├─────────────────────────────────────────┤
│  Persistent Layer                      │
│  ├─ PostgreSQL (主数据存储)               │
│  └─ Redis (分布式锁/限流)              │
└─────────────────────────────────────────┘
```

### 中间件执行顺序

中间件从内到外执行（最后添加的最先执行）：

1. **请求体限制** - 10MB 最大请求体 (防止 DoS)
2. **压缩** - 响应压缩 (Gzip/Brotli)
3. **CORS** - 跨域资源共享
4. **Tracing** - 请求/响应日志
5. **Panic 捕获** - 捕获 panic 并返回 500 错误
6. **认证** - JWT 令牌验证（受保护路由）
7. **速率限制** - 基于 Redis 的分布式限流（认证端点）

### 速率限制

系统在多层实现速率限制：

- **应用层 (Redis 分布式限流)**: 基于 `crates/distributed-ratelimit` 的中间件，支持 IP 级别和邮箱级别的双重限流策略，分别用于认证端点防暴力破解。各端点配置了独立配额（如登录端点：IP 级别 20 次/10分钟，邮箱级别 5 次/10分钟）。
- **反向代理层 (Nginx)**: Nginx 配置中实现的基础限流：
  - **认证端点** (`/api/public/auth/`): 5 请求/分钟 (防止暴力破解)
  - **通用 API** (`/api/`): 60 请求/分钟 (防止资源耗尽)

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
- ✅ JWT 认证与令牌验证
- ✅ 输入验证 (邮箱、密码)
- ✅ 配置加载和覆盖
- ✅ API 端点集成测试
- ✅ 用户 CRUD 操作
- ✅ 密码重置流程
- ✅ 邮箱验证流程
- ✅ 分布式限流
- ✅ 前后端集成测试

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
