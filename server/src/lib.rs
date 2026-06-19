pub mod bootstrap;
pub mod handlers;
pub mod middlewares;
pub mod migrations;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod utils;
pub use utils::snowflake;

use redis::Client as RedisClient;
use std::sync::Arc;
pub use utils::AppConfig;
pub use utils::db_router::AutoRouter;

/// Application shared state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<AutoRouter>,
    pub redis: Option<RedisClient>,
    pub config: Arc<AppConfig>,
    pub email: emailserver::EmailService,
}
