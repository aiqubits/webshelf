# 开发指南

---

## 目录

- [运行时切换：Axum ↔ Salvo](#运行时切换axum--salvo)
- [添加新的 API 端点](#添加新的-api-端点)
- [添加新的数据库表](#添加新的数据库表)
- [添加新的 Crate](#添加新的-crate)
- [使用 AutoRouter 读写分离](#使用-autorouter-读写分离)
- [使用缓存、限流和分布式锁](#使用缓存限流和分布式锁)
- [扩展功能 · 完整示例](#扩展功能--完整示例)
- [配置体系](#配置体系)
- [测试](#测试)
- [贡献指南](#贡献指南)

---

## 运行时切换：Axum ↔ Salvo

WebShelf 通过 `webshelf-runtime` crate 抽象运行时，业务代码（routes / handlers / services）完全不感知底层框架。

### 原理

```
Runtime trait (crates/webshelf-runtime/src/runtime.rs)
  ├── AxumRuntime<S> (crates/webshelf-axum/src/lib.rs)  — feature=webshelf-axum (default)
  └── SalvoRuntime<S> (crates/webshelf-salvo/src/lib.rs) — feature=webshelf-salvo
```

`server/lib.rs` 中通过 feature flag 设定类型别名：

```rust
#[cfg(feature = "webshelf-salvo")]
pub type AppRuntime = SalvoRuntime<AppState>;
#[cfg(not(feature = "webshelf-salvo"))]
pub type AppRuntime = AxumRuntime<AppState>;
```

### 切换命令

```bash
# 默认 Axum
cargo run --package webshelf-server

# 切换 Salvo
cargo run --package webshelf-server \
  --no-default-features --features webshelf-salvo
```

切换后，routes / handlers / services 代码 **零修改**。

### Handler 签名

所有 handler 使用统一的、框架无关的签名：

```rust
use webshelf_runtime::types::{UnifiedRequest, Response, HttpError};

async fn my_handler(req: UnifiedRequest) -> Result<Response, HttpError> {
    // req.extract::<T>() 用于提取 JSON 体、查询参数等
    // 返回 Response::json() / Response::empty()
}
```

### Route 注册（通用模板）

```rust
use webshelf_runtime::Runtime;

fn build_routes<R: Runtime<State = AppState>>() -> R::Router {
    let router = R::new_router();

    let auth_routes = R::new_router()
        .route("/login", post(login_handler))
        .route("/register", post(register_handler));

    R::nest(router, "/api/public/auth", auth_routes)
}
```

两种运行时下的具体实现见：
- [server/src/bootstrap/axum.rs](../server/src/bootstrap/axum.rs) — CORS、中间件栈、路由挂载
- [server/src/bootstrap/salvo.rs](../server/src/bootstrap/salvo.rs) — Salvo 等效实现

---

## 添加新的 API 端点

### 步骤

1. **创建 handler** — 在 `server/src/handlers/` 中新增函数
2. **定义路由** — 在 `server/src/routes/` 中关联 handler
3. **注册路由** — 在 `server/src/routes/` 的入口函数中 `nest` 到总路由
4. **编写测试** — 在 `server/tests/` 或对应 crate 的 tests 目录

### 示例：添加 `GET /api/health/live`

```rust
// 1. server/src/handlers/health.rs
use webshelf_runtime::types::{UnifiedRequest, Response, HttpError};

pub async fn liveness(_req: UnifiedRequest) -> Result<Response, HttpError> {
    Ok(Response::json(&serde_json::json!({"status": "alive"})))
}

// 2. server/src/routes/health.rs
use crate::handlers::health::*;
use webshelf_runtime::Runtime;

pub fn routes<R: Runtime<State = AppState>>() -> R::Router {
    R::new_router()
        .route("/live", get(liveness))
}
```

---

## 添加新的数据库表

### 步骤

1. 在 `server/migrations/` 创建 SQL 迁移文件，命名 `NNN_description.sql`
2. 运行 `cargo run` 自动执行新迁移
3. 在 `server/src/repositories/` 中创建 SeaORM Entity

### 示例：创建 `books` 表

**migrations/002_create_books_table.sql**:

```sql
CREATE TABLE IF NOT EXISTS books (
    id BIGINT PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    author VARCHAR(255) NOT NULL,
    user_id BIGINT NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_books_user_id ON books(user_id);
```

**repositories/book.rs**:

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "books")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub title: String,
    pub author: String,
    pub user_id: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl ActiveModelBehavior for ActiveModel {}
```

---

## 添加新的 Crate

项目遵循"一次做好"原则：当功能被多处共享时，提取为工作空间 crate。

### 现有 crates 一览

| Crate | 位置 | 用途 |
|-------|------|------|
| `webshelf-runtime` | `crates/webshelf-runtime/` | Runtime trait + 共享类型 |
| `webshelf-axum` | `crates/webshelf-axum/` | Axum 运行时适配器 |
| `webshelf-salvo` | `crates/webshelf-salvo/` | Salvo 运行时适配器 |
| `distributed-ratelimit` | `crates/distributed-ratelimit/` | Redis 分布式限流 |
| `emailserver` | `crates/emailserver/` | SMTP 邮件发送 |
| `wechat-api` | `crates/wechat-api/` | 微信公众平台 SDK |
| `i18n` | `crates/i18n/` | 国际化（过程宏） |

### 添加新 crate 的流程

```bash
# 1. 在 crates/ 下创建目录
mkdir -p crates/my-crate/src

# 2. 创建 Cargo.toml，版本号与 workspace 一致
# 3. 在 workspace Cargo.toml 中注册
# 4. 在 server/Cargo.toml 中添加依赖
```

---

## 使用 AutoRouter 读写分离

`AutoRouter` 实现了 SeaORM 的 `ConnectionTrait` + `TransactionTrait`，对业务代码**完全透明**。

### 路由规则

| 操作 | 目标 | 说明 |
|------|------|------|
| `execute()` / `execute_unprepared()` | 写库 | INSERT / UPDATE / DELETE |
| `query_one()` / `query_all()` | 读库 | SELECT 查询 |
| 含 `FOR UPDATE / FOR SHARE` 的 query | 写库 | 行级锁操作 |
| 含 `INSERT/UPDATE ... RETURNING` 的 query | 写库 | SQL 模板检测 |
| `begin()` / `transaction()` | 写库 | 事务全部在主库 |

### 配置

```toml
# config.toml
database_url = "postgres://user:pass@primary:5432/webshelf"
database_read_urls = [
    "postgres://user:pass@replica1:5432/webshelf",
    "postgres://user:pass@replica2:5432/webshelf",
]

[database_routing]
strategy = "round_robin"       # round_robin | random | weighted
retry_attempts = 2             # 每个读库最多重试次数
circuit_break_ms = 30000       # 熔断时长
fallback_to_write = true       # 读库全熔断时降级写库
health_check_interval_secs = 15
```

### 代码中使用

```rust
// 完全透明，和普通 SeaORM 用法一致
use sea_orm::*;

let user = Entity::find_by_id(id)
    .one(&state.db)           // → AutoRouter → 读库
    .await?;

let model = ActiveModel { .. };
model.insert(&state.db)        // → AutoRouter → 写库
    .await?;
```

---

## 使用缓存、限流和分布式锁

三者共享同一个 `redis::Client` 实例，从 `state.cache.redis_client()` 获取。

### CacheService

```rust
// 基本缓存
let val: Option<MyData> = state.cache.get("key").await?;
state.cache.set("key", &my_data, Duration::from_secs(3600)).await?;

// 缓存穿透保护：miss 时回填，null 值标记为负缓存
let data = state.cache.get_or_insert("key", Duration::from_secs(60), || async {
    fetch_data_from_db().await
}).await?;

// 缓存击穿保护：分布式锁防多副本同时回源
let data = state.cache.get_or_insert_with_lock(
    "hot_key",
    Duration::from_secs(60),    // 缓存 TTL
    5,                          // 锁持有时间（秒）
    Duration::from_millis(100), // 重试间隔
    50,                         // 最大重试次数
    || async { compute_expensive().await }
).await?;
```

### 分布式限流

限流中间件在 [server/src/middlewares/mod.rs](../server/src/middlewares/mod.rs) 中定义，基于 `distributed-ratelimit` crate：

```rust
use distributed_ratelimit::RateLimiter;

// 为每个认证端点定义独立配额
let login_limiter = RateLimiter::new(
    redis_client,
    "rl:auth:login",   // Redis key 前缀
    20,                  // 窗口内最大请求数（IP 级别）
    600,                 // 窗口大小（秒）
);
```

当前每个认证端点的配额（[server/src/routes/auth.rs](../server/src/routes/auth.rs)）：

| 端点 | IP 级别 | 邮箱级别 |
|------|---------|----------|
| `/login` | 20/10min | 5/10min |
| `/register` | 10/10min | - |
| `/forgot-password` | 5/10min | - |
| `/verify-email` | 20/10min | - |
| `/refresh` | 30/10min | - |

### 分布式锁

```rust
use crate::services::lock::{LockGuard, acquire_lock};

// fail-open 方式：Redis 不可用时返回 None
if let Some(guard) = LockGuard::acquire(
    state.cache.redis_client(),
    "lock:resource",
    10,  // TTL（秒）
).await? {
    // 获得锁，做临界操作
    // guard 在 Drop 时自动释放
}

// fail-close 方式：Redis 不可用时返回 Err
let (acquired, _value) = acquire_lock(
    state.cache.redis_client().as_ref(),
    "lock:critical",
    10,    // TTL（秒）
    3,     // 重试次数
    Duration::from_millis(200), // 重试间隔
).await?;
```

---

## 扩展功能 · 完整示例

### 添加一个带缓存的分页列表端点

```rust
// 1. handlers/book.rs
use webshelf_runtime::types::*;

pub async fn list_books(req: UnifiedRequest) -> Result<Response, HttpError> {
    let state = req.state::<AppState>()?;
    let page: u64 = req.query("page").unwrap_or(1);
    let per_page: u64 = req.query("per_page").unwrap_or(20);

    let cache_key = format!("books:list:{}:{}", page, per_page);
    let books = state.cache.get_or_insert(&cache_key, Duration::from_secs(30), || async {
        Book::find()
            .paginate(&state.db, per_page)
            .fetch_page(page - 1)
            .await
            .map_err(|e| e.to_string())
    }).await.map_err(|_| HttpError::internal("Failed to fetch books"))?;

    Ok(Response::json(&books))
}

// 2. routes/book.rs
pub fn routes<R: Runtime<State = AppState>>() -> R::Router {
    R::new_router()
        .route("/books", get(list_books))
}

// 3. 在 routes/api.rs 中 nest
// R::nest(router, "/api", book::routes::<R>());
```

### 使用 Snowflake ID

实体创建时使用 Snowflake 生成 ID：

```rust
use crate::utils::snowflake::SnowflakeGenerator;

let id = state.snowflake.generate();
let model = book::ActiveModel {
    id: Set(id.into()),
    title: Set(input.title),
    author: Set(input.author),
    ..Default::default()
};
model.insert(&state.db).await?;
```

---

## 配置体系

WebShelf 支持三层配置来源，优先级从低到高：

1. **config.toml** — 静态配置文件
2. **Kubernetes Secret / ConfigMap** — 容器环境
3. **环境变量** — 最高优先级（前缀 `WEBSHELF_`）

### config.toml 结构

```toml
database_url = "postgres://..."
database_read_urls = []           # 可选，启用读写分离
redis_url = "redis://..."
jwt_secret = "..."
jwt_expiry_seconds = 3600
jwt_remember_me_expiry_days = 30
cookie_secure = false

[server]
host = "0.0.0.0"
port = 3000
allowed_origins = ["http://localhost:8080"]

[database]
max_connections = 20
min_connections = 5

[database_routing]                # 可选，读写分离行为
strategy = "round_robin"
retry_attempts = 2
circuit_break_ms = 30000
fallback_to_write = true
health_check_interval_secs = 15

[logging]
level = "info"
```

### 环境变量映射

```
config.toml 路径           → 环境变量
database_url               → WEBSHELF_DATABASE_URL
database_read_urls         → WEBSHELF_DATABASE_READ_URLS
server.host                → WEBSHELF_SERVER__HOST
server.port                → WEBSHELF_SERVER__PORT
server.allowed_origins     → WEBSHELF_SERVER__ALLOWED_ORIGINS
database.max_connections   → WEBSHELF_DATABASE__MAX_CONNECTIONS
```

### CLI 参数覆盖

```bash
cargo run --package webshelf-server -- \
  --env production \
  --host 0.0.0.0 --port 3000 \
  --log-level debug \
  --jwt-secret "..."
```

---

## 测试

### 运行测试

```bash
# 全部测试
cargo test --workspace

# 仅服务端
cargo test --package webshelf-server

# 仅客户端 API
cargo test --package client-api

# 集成测试（需要 PostgreSQL 和 Redis 运行中）
cargo test --test integration_tests -- --test-threads=1
```

### 测试覆盖范围

- 密码哈希和验证
- JWT 认证与令牌验证
- 输入验证（邮箱、密码）
- 配置加载和覆盖
- API 端点集成测试
- 用户 CRUD 操作
- 密码重置流程
- 邮箱验证流程
- 分布式限流
- 前后端集成测试

### 编写测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_password_hashing() {
        let hash = hash_password("TestPass123").unwrap();
        assert!(verify_password("TestPass123", &hash).unwrap());
        assert!(!verify_password("WrongPass", &hash).unwrap());
    }

    #[tokio::test]
    async fn test_email_validation() {
        assert!(validate_email("user@example.com").is_ok());
        assert!(validate_email("invalid-email").is_err());
    }
}
```

---

## 贡献指南

### 环境准备

```bash
pip install pre-commit   # 或 brew install pre-commit
pre-commit install       # 在项目根目录执行
```

pre-commit hooks 按顺序执行：
1. **测试** — 运行 server/web/client-api 测试
2. **Clippy** — 对所有 crate 静态分析
3. **Fmt** — 检查代码格式

### 提交步骤

1. Fork 仓库
2. 创建特性分支：`git checkout -b feature/amazing-feature`
3. 提交更改：`git commit -m 'Add amazing feature'`
4. 推送到分支：`git push origin feature/amazing-feature`
5. 创建 Pull Request

### 代码规范

- **不变更已有测试**：新增功能需补充对应测试
- **遵循事实标准**：首选社区标准库，不重复造轮子
- **最少封装**：`Runtime` trait 每个方法对应一个操作，不隐藏框架细节
- **AI Coding 友好**：类型清晰、命名规范、Router 构建集中管理（见 `routes/` 下的统一入口）

---

## 项目结构参考

完整项目结构见 [架构概览](architecture.md#项目结构)。