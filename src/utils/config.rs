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
    #[serde(default)]
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

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Server host address
    #[serde(default = "default_host")]
    pub host: String,

    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitConfig {
    /// Requests per second
    #[serde(default = "default_rate_per_second")]
    pub per_second: u64,

    /// Burst size
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
}

// Default database connection URL
fn default_database_url() -> String {
    "postgres://postgres:password@localhost:5432/postgres".to_string()
}

// Default JWT secret key
fn default_jwt_secret() -> String {
    "your-super-secret-key-change-in-production".to_string()
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

fn default_rate_per_second() -> u64 {
    2
}

fn default_burst_size() -> u32 {
    5
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: default_rate_per_second(),
            burst_size: default_burst_size(),
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
        .add_source(Environment::with_prefix("WEBSHELF").prefix_separator("_").separator("__"))
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

        let rate_limit = RateLimitConfig::default();
        assert_eq!(rate_limit.per_second, 2);
        assert_eq!(rate_limit.burst_size, 5);
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
    fn test_default_rate_per_second() {
        assert_eq!(default_rate_per_second(), 2);
    }

    #[test]
    fn test_default_burst_size() {
        assert_eq!(default_burst_size(), 5);
    }

    #[test]
    fn test_server_config_clone() {
        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
        };
        let cloned = config.clone();
        assert_eq!(config.host, cloned.host);
        assert_eq!(config.port, cloned.port);
    }

    #[test]
    fn test_rate_limit_config_clone() {
        let config = RateLimitConfig {
            per_second: 10,
            burst_size: 20,
        };
        let cloned = config.clone();
        assert_eq!(config.per_second, cloned.per_second);
        assert_eq!(config.burst_size, cloned.burst_size);
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
    fn test_rate_limit_config_debug() {
        let config = RateLimitConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("RateLimitConfig"));
    }

    #[test]
    fn test_app_config_clone() {
        let config = AppConfig {
            database_url: "postgres://localhost".to_string(),
            redis_url: "redis://localhost".to_string(),
            jwt_secret: "secret".to_string(),
            jwt_expiry_seconds: 7200,
            server: ServerConfig::default(),
            rate_limit: RateLimitConfig::default(),
        };
        let cloned = config.clone();
        assert_eq!(config.database_url, cloned.database_url);
        assert_eq!(config.jwt_expiry_seconds, cloned.jwt_expiry_seconds);
    }
}
