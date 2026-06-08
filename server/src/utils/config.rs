use anyhow::{Context, Result};
use config::{Config, Environment, File};
use serde::Deserialize;

/// Application configuration structure
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// Database connection URL
    #[serde(default = "default_database_url")]
    pub database_url: String,

    /// Redis connection URL for distributed locking
    #[serde(default = "default_redis_url")]
    pub redis_url: String,

    /// JWT secret key for token signing
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,

    /// JWT token expiration time in seconds (default: 3600)
    #[serde(default = "default_jwt_expiry")]
    pub jwt_expiry_seconds: u64,

    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Database connection pool configuration
    #[serde(default)]
    pub database: DatabaseConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Server host address
    #[serde(default = "default_host")]
    pub host: String,

    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Allowed CORS origins (empty = allow Any, but logs a warning in production)
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

/// Database connection pool configuration
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    /// Maximum number of connections in the pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Minimum number of idle connections to maintain
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
}

fn default_max_connections() -> u32 {
    10
}
fn default_min_connections() -> u32 {
    1
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
        }
    }
}

// Default database connection URL
fn default_database_url() -> String {
    "postgres://postgres:CHANGE_ME_POSTGRES_PASSWORD@localhost:5432/webshelf".to_string()
}

// Default Redis connection URL
fn default_redis_url() -> String {
    "redis://:CHANGE_ME_REDIS_PASSWORD@localhost:6379".to_string()
}

// Default JWT secret key — must be replaced before deployment
fn default_jwt_secret() -> String {
    "REPLACE_ME_WITH_A_STRONG_SECRET".to_string()
}

// Default value functions
fn default_jwt_expiry() -> u64 {
    3600
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            allowed_origins: Vec::new(),
        }
    }
}

/// Load application configuration from file and environment variables
///
/// Configuration is loaded in the following order (later sources override earlier):
/// 1. Base config file (config_path)
/// 2. Environment-specific config file (config.{env}.toml)
/// 3. Environment variables with WEBSHELF_ prefix
pub fn load_config(config_path: &str, env: &str) -> Result<AppConfig> {
    let settings = Config::builder()
        // Load base configuration file
        .add_source(File::with_name(config_path).required(false))
        // Load environment-specific configuration
        .add_source(File::with_name(&format!("config.{}", env)).required(false))
        // Load environment variables with WEBSHELF_ prefix
        .add_source(
            Environment::with_prefix("WEBSHELF")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()
        .context("Failed to build configuration")?;

    settings
        .try_deserialize()
        .context("Failed to deserialize configuration")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let server = ServerConfig::default();
        assert_eq!(server.host, "0.0.0.0");
        assert_eq!(server.port, 3000);
        assert!(server.allowed_origins.is_empty());
    }

    #[test]
    fn test_default_jwt_expiry() {
        assert_eq!(default_jwt_expiry(), 3600);
    }

    #[test]
    fn test_default_host() {
        assert_eq!(default_host(), "0.0.0.0");
    }

    #[test]
    fn test_default_port() {
        assert_eq!(default_port(), 3000);
    }

    #[test]
    fn test_server_config_clone() {
        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            allowed_origins: vec!["http://localhost:3000".to_string()],
        };
        let cloned = config.clone();
        assert_eq!(config.host, cloned.host);
        assert_eq!(config.port, cloned.port);
        assert_eq!(config.allowed_origins, cloned.allowed_origins);
    }

    #[test]
    fn test_server_config_debug() {
        let config = ServerConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ServerConfig"));
        assert!(debug_str.contains("0.0.0.0"));
        assert!(debug_str.contains("3000"));
    }

    #[test]
    fn test_app_config_clone() {
        let config = AppConfig {
            database_url: "postgres://localhost".to_string(),
            redis_url: "redis://localhost".to_string(),
            jwt_secret: "secret".to_string(),
            jwt_expiry_seconds: 7200,
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
        };
        let cloned = config.clone();
        assert_eq!(config.database_url, cloned.database_url);
        assert_eq!(config.jwt_expiry_seconds, cloned.jwt_expiry_seconds);
    }
}
