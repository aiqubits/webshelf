//! Framework-specific re-exports, unified for both axum and salvo modes.
//!
//! Routes should import from this module instead of using `#[cfg]` directly,
//! reducing duplication and making the route registration code cleaner.

use crate::AppRouter;
use distributed_ratelimit::RedisRateLimiter;
use webshelf_runtime::RateLimitGuard;

// ── Unified routing method re-exports ────────────────────────

#[cfg(not(feature = "webshelf-salvo"))]
pub use webshelf_axum::{delete, get, post, put};

#[cfg(feature = "webshelf-salvo")]
pub use webshelf_salvo::{delete, get, post, put};

// ── Unified apply_rate_limit ─────────────────────────────────

/// Apply rate-limit middleware using the active framework's middleware API.
pub fn apply_rate_limit(route: AppRouter, guard: RateLimitGuard) -> AppRouter {
    #[cfg(not(feature = "webshelf-salvo"))]
    return webshelf_axum::with_rate_limit_layer(route, guard);
    #[cfg(feature = "webshelf-salvo")]
    return webshelf_salvo::with_rate_limit_hoop(route, guard);
}

// ── Unified admin guard application ──────────────────────────

/// Apply admin-role middleware guard to a router.
///
/// Axum mode: uses `route_layer(from_fn(require_admin))`.
/// Salvo mode: uses `.hoop(RequireAdmin)`.
pub fn apply_admin_guard(router: AppRouter) -> AppRouter {
    #[cfg(not(feature = "webshelf-salvo"))]
    {
        use crate::middlewares::require_admin;
        router.route_layer(webshelf_axum::from_fn(require_admin))
    }
    #[cfg(feature = "webshelf-salvo")]
    {
        use crate::middlewares::RequireAdmin;
        router.hoop(RequireAdmin)
    }
}

// ── Unified rate limiter initialization (shared by both modules) ──

/// Create a `RedisRateLimiter` from the cache service's redis client.
pub fn create_rate_limiter(cache: &crate::services::CacheService) -> RedisRateLimiter {
    match cache.redis_client() {
        Some(client) => {
            tracing::info!("Rate limiter: sharing redis::Client with CacheService.");
            RedisRateLimiter::new(
                client.clone(),
                distributed_ratelimit::RateLimitConfig::default(),
            )
        }
        None => {
            tracing::warn!(
                "Redis not available behind CacheService — login rate limiting is disabled. \
                 Set WEBSHELF_REDIS_URL or redis_url in config.toml to enable."
            );
            RedisRateLimiter::disabled(distributed_ratelimit::RateLimitConfig::default())
        }
    }
}
