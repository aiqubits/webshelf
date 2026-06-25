//! Framework-agnostic middleware types and re-exports from the active adapter.

pub(crate) const JWT_COOKIE: &str = "webshelf_jwt";
pub(crate) const REFRESH_COOKIE: &str = "webshelf_refresh";
pub(crate) const EXPIRY_COOKIE: &str = "webshelf_exp";

// Re-export AuthUser and RateLimitGuard from webshelf-runtime
pub use webshelf_runtime::{AuthUser, RateLimitGuard};

// Re-export generate_token for backward compatibility
pub use crate::utils::jwt::generate_token;

// Axum mode: re-export middleware from the adapter
#[cfg(not(feature = "webshelf-salvo"))]
pub use webshelf_axum::middleware::{
    auth_middleware, panic_middleware, rate_limit_middleware, require_admin,
};

// Salvo mode: re-export middleware from the adapter
#[cfg(feature = "webshelf-salvo")]
pub use webshelf_salvo::middleware::{AuthMiddleware, RateLimitMiddleware, RequireAdmin};
