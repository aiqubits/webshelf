pub mod bootstrap;
pub mod handlers;
pub mod middlewares;
pub mod migrations;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod utils;
pub use utils::snowflake;

use std::sync::Arc;
pub use utils::AppConfig;
pub use utils::db_router::AutoRouter;

use crate::services::CacheService;

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
