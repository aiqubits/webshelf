pub mod bootstrap;
pub mod middlewares;
pub mod migrations;
pub mod repositories;
pub mod handlers;
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
    pub redis: Option<RedisClient>,
    pub config: Arc<AppConfig>,
}
