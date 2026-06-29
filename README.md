[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/aiqubits/webshelf/pulls)
[![Live Demo](https://img.shields.io/badge/demo-live-success)](https://www.openpick.org/webshelf)

# WebShelf

**The best way to develop your web service with one click.**

[English](README.md) | [简体中文](README.zh-CN.md)

[Features](#features) • [Quick Start](#quick-start) • [Docs](#docs) • [License](#license)

## Overview

WebShelf is a production-ready full-stack system framework built on a **pure Rust stack** — multi-runtime backend (Axum/Salvo), multi-platform frontend (Dioxus Web/Desktop/Mobile), SeaORM async ORM, and Redis distributed trifecta (caching/rate-limiting/locking). From code to deployment, all industry standards, AI Coding friendly.

## Features

### Full-Stack Rust Tech Stack
- **Multi-engine Backend** — Via the `webshelf-runtime` abstraction layer, the same business code supports both [Axum](https://github.com/tokio-rs/axum) and [Salvo](https://github.com/salvo-rs/salvo) runtimes via feature flags, zero code changes
- **Multi-platform Frontend** — [Dioxus](https://dioxuslabs.com/) 0.7, a single UI component library (`app/ui`) driving Web/Desktop/Mobile
- **Client SDK** — `client-api` crate encapsulating authentication, requests, and shared types between frontend and backend
- **Minimal Abstraction** — Handlers use framework-native types directly; the `Runtime` trait only abstracts route building and server startup without hiding framework capabilities

### Master-Replica Read/Write Split Database
- **`AutoRouter`** — Implements SeaORM `ConnectionTrait` + `TransactionTrait`; INSERT/UPDATE/DELETE auto-routes to the write database, SELECT auto-routes to read replicas
- **Multi-strategy Load Balancing** — round_robin / random / weighted strategies for read replica selection
- **Circuit Breaker + Health Check** — Automatic circuit breaking on failure, background periodic health checks, graceful fallback to write database
- **One-click K8s Deployment** — CloudNativePG 1 primary 2 replicas cluster, automatic failover, `-rw`/`-ro` Service endpoints
- **Docker Compose Dual-mode** — [`docker-compose.yml`](docker-compose.yml) for single-instance PostgreSQL; [`docker-compose.replicas.yml`](docker-compose.replicas.yml) for 1 primary + 2 streaming replicas with read/write split

### Redis Data Interface Caching + Distributed Rate Limiting + Distributed Locking
- **Unified `CacheService`** — bb8 connection pool, `get_or_insert` automatic backfill, `get_or_insert_with_lock` cache stampede protection (distributed lock prevents thundering herd)
- **Graceful Degradation** — All operations silently no-op when Redis is unavailable; service does not crash
- **`distributed-ratelimit`** — Fixed window rate limiting, dual IP-level + email-level strategy, precise per-auth-endpoint
- **`LockGuard`** — Lua script atomic release, `Drop` auto-unlock, fail-open / fail-close dual strategy

### Web Framework Runtime Switching
- **`Runtime` trait** — Defines seven operations: `Router` / `MethodRouter` / `with_route` / `nest` / `merge` / `with_state` / `serve`
- **`webshelf-axum`** / **`webshelf-salvo`** adapters — Each implements `Runtime`, switched via Cargo feature `default = ["webshelf-axum"]`
- **Unified Handler Signature** — `async fn(UnifiedRequest) -> Result<Response, HttpError>`, framework-agnostic

### Security System
- **Argon2id** password hashing — Automatic salting, KDF standard
- **JWT** — HS256 signing, `token_version` version control, remember-me support (30 days), Refresh Token rotation
- **Input Validation** — RFC 5322 email validation, password strength (upper/lowercase + digits + 8 chars minimum), length limits
- **HTTP Security Headers** — HSTS / X-Frame-Options / X-Content-Type-Options / CSP

### Distributed Infrastructure
- **Snowflake ID** — Twitter algorithm, DB-coordinated worker_id, lock-free atomic generation, JSON serialized as string (JS precision safe)
- **Internationalization i18n** — `crates/i18n`, procedural macro for automatic field translation
- **WeChat Integration** — `wechat-api` crate, official account verification code login

### AI Coding Friendly
- **Industry Standards** — SeaORM, Axum/Salvo, Tracing, Serde, Chrono, no proprietary frameworks
- **Minimal Encapsulation** — Each `Runtime` trait method maps to one operation, no hidden framework details
- **Comprehensive Documentation & Testing** — Unit + integration tests, complete API documentation

---

## Quick Start

```bash
# 1. Clone and enter
git clone https://github.com/aiqubits/webshelf.git
cd webshelf

# 2. Start PostgreSQL and Redis (Docker)
docker network create webshelf-net 2>/dev/null || true
docker run -d --network webshelf-net --name webshelf-postgres-dev \
  -e POSTGRES_PASSWORD=devpassword -e POSTGRES_DB=webshelf \
  -p 5432:5432 postgres:16-alpine
docker run -d --network webshelf-net --name webshelf-redis-dev \
  -p 6379:6379 redis:7-alpine

# 3. Configure
cp config.toml.example config.toml

# 4. Start (default Axum runtime)
cargo run -p webshelf-server -- --env development --log-level debug

# 5. Switch to Salvo runtime
cargo run -p webshelf-server --features webshelf-salvo -- --env development --log-level debug
```

---

## Docs

| Document | Description |
|----------|-------------|
| [Architecture Overview](docs/architecture.md) | Runtime abstraction, read/write split, Redis trifecta, deployment architecture, API docs |
| [Deployment Guide](docs/deployment.md) | Local dev, Docker Compose, K8s (including CNP cluster), configuration management |
| [Development Guide](docs/development.md) | Extending endpoints, DB migrations, runtime switching, contribution guide |

---

## License

MIT License - see [LICENSE](LICENSE).

## Author

- **aiqubits** - [aiqubits@hotmail.com](mailto:aiqubits@hotmail.com)