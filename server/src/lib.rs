// handlers/routes/middlewares — always compiled (framework-agnostic)
pub mod handlers;
pub mod middlewares;
pub mod routes;

// salvo 目录已删除 — 所有 salvo 特定代码已下沉到 webshelf-salvo crate
pub mod bootstrap;
pub mod migrations;
pub mod repositories;
pub mod services;
pub mod utils;
pub use utils::snowflake;

use std::sync::Arc;
pub use utils::AppConfig;
pub use utils::db_router::AutoRouter;

use crate::services::CacheService;
use sea_orm::EntityTrait;

use async_trait::async_trait;
use webshelf_runtime::MiddlewareState;

// 新风格 handler 的类型别名（框架无关）
#[cfg(not(feature = "webshelf-salvo"))]
pub type ServerRequest = webshelf_axum::UnifiedRequest;
#[cfg(feature = "webshelf-salvo")]
pub type ServerRequest = webshelf_salvo::UnifiedRequest;

/// Ensure at least one framework feature is enabled.
#[cfg(not(any(feature = "webshelf-axum", feature = "webshelf-salvo")))]
compile_error!(
    "At least one framework feature must be enabled: 'webshelf-axum' (default) or 'webshelf-salvo'"
);

/// Application shared state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<AutoRouter>,
    /// Unified Redis-backed cache service (bb8 pool).
    /// Gracefully degrades to no-op when Redis is unavailable.
    pub cache: CacheService,
    pub config: Arc<AppConfig>,
    pub email: emailserver::EmailService,
}

#[async_trait]
impl MiddlewareState for AppState {
    fn jwt_secret(&self) -> &str {
        &self.config.jwt_secret
    }

    fn cookie_secure(&self) -> bool {
        self.config.cookie_secure
    }

    async fn check_token_version(&self, user_id: i64, token_version: i32) -> Result<(), String> {
        verify_token_version(&self.db, &self.cache, user_id, token_version)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Verify token_version matches the user's current version.
/// Uses Redis cache (30s TTL) with DB fallback.
async fn verify_token_version(
    db: &AutoRouter,
    cache: &CacheService,
    user_id: i64,
    token_version: i32,
) -> anyhow::Result<()> {
    use crate::repositories::user::Entity as UserEntity;
    use anyhow::Context;

    let cache_key = format!("user:token_version:{}", user_id);

    // 1. Try cache first
    if let Ok(Some(cached_version)) = cache.get::<i32>(&cache_key).await {
        if cached_version == token_version {
            return Ok(());
        }
        return Err(anyhow::anyhow!(
            "Token version mismatch (token was invalidated)"
        ));
    }

    // 2. Cache miss — query DB (write DB for read-your-writes consistency)
    let user = UserEntity::find_by_id(user_id)
        .one(db.write_conn())
        .await
        .context("Failed to query user for token version check")?
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

    // 3. Cache the result (best-effort, 30s TTL)
    let ttl = std::time::Duration::from_secs(30);
    let _ = cache.set(&cache_key, &user.token_version, ttl).await;

    if user.token_version != token_version {
        return Err(anyhow::anyhow!(
            "Token version mismatch (token was invalidated by password change)"
        ));
    }

    Ok(())
}

pub use webshelf_runtime::Runtime;

// ── Framework-agnostic type re-exports ─────────
// Handler functions: re-exported from the active adapter so that
// server route builders (routes/api.rs, routes/auth.rs) can use
// them uniformly via `crate` path.
#[cfg(not(feature = "webshelf-salvo"))]
pub use webshelf_axum::{UnifiedError, UnifiedResponse, delete, get, post, put};

// ── Axum-specific types (axum mode only) ────────────
// Types needed by bootstrap.rs, type aliases, and tests.
// These are framework-specific (CorsLayer, TraceLayer, etc.)
// and should NOT be used by handlers or routes.
#[cfg(not(feature = "webshelf-salvo"))]
pub use webshelf_axum::{AxumRuntime, response_to_axum};

// ── Salvo mode: export SalvoRuntime, routing functions, etc. ────
#[cfg(feature = "webshelf-salvo")]
pub use webshelf_salvo::{SalvoRouter, SalvoRuntime, delete, get, post, put};

// ── Feature flag: select framework adapter ────────

#[cfg(feature = "webshelf-salvo")]
pub type AppRuntime = SalvoRuntime<AppState>;

#[cfg(feature = "webshelf-salvo")]
pub type AppRouter = <AppRuntime as Runtime>::Router;

#[cfg(not(feature = "webshelf-salvo"))]
pub type AppRuntime = AxumRuntime<AppState>;

#[cfg(not(feature = "webshelf-salvo"))]
pub type AppRouter = <AppRuntime as Runtime>::Router;
