//! Shared test utilities — framework-agnostic helpers for integration tests.
//!
//! Some shared functions may not be used by every test variant (axum vs salvo).
//! `#[allow(dead_code)]` is applied to suppress warnings in single-variant compilations.

#![allow(dead_code)]

use std::sync::Arc;

// Framework-specific test harnesses — use #[cfg] to select the active one.
#[cfg(not(feature = "webshelf-salvo"))]
pub mod axum;

#[cfg(feature = "webshelf-salvo")]
pub mod salvo;

/// Generate a unique test email using a nanosecond timestamp.
pub fn unique_email(label: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}_{}@example.com", label, ts)
}

/// Load the test configuration from config.toml (development environment).
pub fn load_test_config() -> webshelf_server::utils::AppConfig {
    webshelf_server::utils::load_config("config.toml", "development")
        .expect("Failed to load config.toml for tests")
}

/// Create a database connection with AutoRouter (single writer, no replicas).
pub async fn create_test_db_and_run_migrations() -> Arc<webshelf_server::AutoRouter> {
    let config = load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    let db = webshelf_server::AutoRouter::single(db);

    webshelf_server::migrations::run_migrations(db.write_conn())
        .await
        .expect("Failed to run migrations");

    webshelf_server::snowflake::init(db.write_conn())
        .await
        .expect("Failed to initialize Snowflake generator");

    db
}

/// Create a cache service from config.
pub async fn create_cache_service() -> webshelf_server::services::CacheService {
    let config = load_test_config();
    webshelf_server::services::CacheService::new(&config.redis_url, config.cache_max_connections)
        .await
}

/// Create a disabled rate limiter for tests.
pub fn disabled_rate_limiter() -> distributed_ratelimit::RedisRateLimiter {
    distributed_ratelimit::RedisRateLimiter::disabled(
        distributed_ratelimit::RateLimitConfig::default(),
    )
}

/// Default email service for tests (unconfigured — never sends real emails).
pub fn default_email_service() -> emailserver::EmailService {
    emailserver::EmailService::new(emailserver::EmailConfig::default())
}

/// Generate a unique table name suffix for parallel test safety.
pub fn unique_table_name(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("_{}_{}", prefix, ts)
}
