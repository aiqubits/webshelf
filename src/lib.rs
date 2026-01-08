pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod utils;

use redis::Client as RedisClient;
use sea_orm::DatabaseConnection;
use std::sync::Arc;
pub use utils::AppConfig;

/// Application shared state
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub redis: RedisClient,
    pub config: Arc<AppConfig>,
}
