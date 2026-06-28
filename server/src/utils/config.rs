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

    /// Redis cache connection pool size (default: 10)
    #[serde(default = "default_cache_max_connections")]
    pub cache_max_connections: u32,

    /// Read replica database URLs (empty = no read-write split, backward compatible)
    #[serde(default)]
    pub database_read_urls: Vec<String>,

    /// Read-write routing configuration
    #[serde(default)]
    pub database_routing: DatabaseRoutingConfig,

    /// Read replica connection pool configuration
    #[serde(default)]
    pub database_read: DatabaseReadConfig,

    /// JWT secret key for token signing
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,

    /// JWT token expiration time in seconds (default: 3600)
    #[serde(default = "default_jwt_expiry")]
    pub jwt_expiry_seconds: u64,

    /// JWT expiration when "remember me" is enabled (default: 2592000 = 30 days)
    #[serde(default = "default_jwt_remember_expiry")]
    pub jwt_remember_expiry_seconds: u64,

    /// Refresh token expiration time in seconds (default: 7776000 = 90 days)
    #[serde(default = "default_refresh_token_expiry")]
    pub refresh_token_expiry_seconds: u64,

    /// Whether to set the Secure flag on auth cookies (default: true).
    /// Set to false for local development over plain HTTP.
    #[serde(default = "default_cookie_secure")]
    pub cookie_secure: bool,

    /// System admin account email (auto-seeded on first boot)
    #[serde(default = "default_system_admin_email")]
    pub system_admin_email: String,

    /// System admin account password (auto-seeded on first boot)
    #[serde(default = "default_system_admin_password")]
    pub system_admin_password: String,

    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Database connection pool configuration
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Email (SMTP) configuration
    #[serde(default)]
    pub email: emailserver::EmailConfig,

    /// WeChat Official Account configuration (optional)
    #[serde(default)]
    pub wechat: WechatAccountConfig,
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

    /// Connection timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,

    /// Idle timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,

    /// Acquire timeout in seconds
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout_secs: u64,
}

fn default_max_connections() -> u32 {
    10
}
fn default_min_connections() -> u32 {
    1
}
fn default_connect_timeout() -> u64 {
    8
}
fn default_idle_timeout() -> u64 {
    600
}
fn default_acquire_timeout() -> u64 {
    30
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout_secs: default_connect_timeout(),
            idle_timeout_secs: default_idle_timeout(),
            acquire_timeout_secs: default_acquire_timeout(),
        }
    }
}

/// Read-write routing configuration
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseRoutingConfig {
    /// Read replica selection strategy: round_robin | random | weighted
    #[serde(default = "default_read_strategy")]
    pub strategy: String,

    /// Weights for each read replica (only applies to weighted strategy)
    #[serde(default)]
    pub read_weights: Vec<u32>,

    /// Extra retry attempts after each read replica has been tried once.
    /// Each extra attempt retries any non-circuit-broken replica (including
    /// those whose circuit breaker has expired since the first attempt).
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: usize,

    /// Circuit breaker duration in milliseconds for a failed read replica
    #[serde(default = "default_circuit_break_ms")]
    pub circuit_break_ms: u64,

    /// Fall back to the write database when no healthy read replicas are available
    #[serde(default = "default_fallback_to_write")]
    pub fallback_to_write: bool,

    /// Background health check interval in seconds (0 = disabled)
    #[serde(default = "default_health_check_interval_secs")]
    pub health_check_interval_secs: u64,
}

impl Default for DatabaseRoutingConfig {
    fn default() -> Self {
        Self {
            strategy: default_read_strategy(),
            read_weights: Vec::new(),
            retry_attempts: default_retry_attempts(),
            circuit_break_ms: default_circuit_break_ms(),
            fallback_to_write: default_fallback_to_write(),
            health_check_interval_secs: default_health_check_interval_secs(),
        }
    }
}

/// Read replica connection pool configuration (independent from the write pool)
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseReadConfig {
    /// Maximum number of connections in the pool
    #[serde(default = "default_read_max_connections")]
    pub max_connections: u32,

    /// Minimum number of idle connections to maintain
    #[serde(default = "default_read_min_connections")]
    pub min_connections: u32,

    /// Connection timeout in seconds
    #[serde(default = "default_read_connect_timeout")]
    pub connect_timeout_secs: u64,

    /// Idle timeout in seconds
    #[serde(default = "default_read_idle_timeout")]
    pub idle_timeout_secs: u64,

    /// Acquire timeout in seconds
    #[serde(default = "default_read_acquire_timeout")]
    pub acquire_timeout_secs: u64,
}

impl Default for DatabaseReadConfig {
    fn default() -> Self {
        Self {
            max_connections: default_read_max_connections(),
            min_connections: default_read_min_connections(),
            connect_timeout_secs: default_read_connect_timeout(),
            idle_timeout_secs: default_read_idle_timeout(),
            acquire_timeout_secs: default_read_acquire_timeout(),
        }
    }
}

// Default database connection URL
fn default_database_url() -> String {
    "postgres://postgres:CHANGE_ME_POSTGRES_PASSWORD@127.0.0.1:5432/webshelf".to_string()
}

// Default Redis connection URL
fn default_redis_url() -> String {
    "redis://:CHANGE_ME_REDIS_PASSWORD@127.0.0.1:6379".to_string()
}

/// Default Redis cache connection pool size
fn default_cache_max_connections() -> u32 {
    10
}

// Default JWT secret key — must be replaced before deployment
fn default_jwt_secret() -> String {
    "REPLACE_ME_WITH_A_STRONG_SECRET".to_string()
}

fn default_system_admin_email() -> String {
    "admin@webshelf.local".to_string()
}

fn default_system_admin_password() -> String {
    "change-me-admin-password".to_string()
}

// Default value functions
fn default_jwt_expiry() -> u64 {
    3600
}

fn default_jwt_remember_expiry() -> u64 {
    2_592_000
}

fn default_refresh_token_expiry() -> u64 {
    7_776_000
}

fn default_cookie_secure() -> bool {
    true
}

fn default_read_strategy() -> String {
    "round_robin".to_string()
}
fn default_retry_attempts() -> usize {
    2
}
fn default_circuit_break_ms() -> u64 {
    30000
}
fn default_fallback_to_write() -> bool {
    true
}
fn default_health_check_interval_secs() -> u64 {
    15
}
fn default_read_max_connections() -> u32 {
    20
}
fn default_read_min_connections() -> u32 {
    2
}
fn default_read_connect_timeout() -> u64 {
    5
}
fn default_read_idle_timeout() -> u64 {
    600
}
fn default_read_acquire_timeout() -> u64 {
    10
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

/// WeChat Official Account configuration (optional).
///
/// When enabled, users can log in via captcha codes obtained from the
/// WeChat Official Account instead of using email + password.
/// All fields must be configured for the feature to be enabled.
#[derive(Debug, Deserialize, Clone)]
pub struct WechatAccountConfig {
    /// Whether the WeChat captcha-login feature is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// A logical identifier for this account (e.g. primary tenant id).
    /// Used as a namespace prefix for cache keys so multiple accounts never
    /// collide.
    #[serde(default)]
    pub account_id: String,

    /// WeChat AppID.
    #[serde(default)]
    pub app_id: String,

    /// WeChat AppSecret.
    #[serde(default)]
    pub app_secret: String,

    /// The Token configured in the MP backend for signature verification.
    #[serde(default)]
    pub token: String,

    /// The EncodingAESKey (43 chars) configured for "safe mode".
    /// Only required when WeChat message encryption mode is set to Safe.
    #[serde(default)]
    pub encoding_aes_key: Option<String>,

    /// The original ID of the official account (e.g. `gh_xxxx`).
    /// Used to route incoming messages to the correct account config.
    #[serde(default)]
    pub original_id: Option<String>,

    /// WeChat message encryption mode: plain, compatible, or safe.
    #[serde(default)]
    pub message_mode: String,

    /// How long a generated captcha stays valid, in seconds. Default 300.
    #[serde(default = "default_captcha_ttl")]
    pub captcha_ttl_secs: u64,

    /// Minimum interval between two captcha requests from the same user,
    /// in seconds. Default 60.
    #[serde(default = "default_resend_cooldown")]
    pub resend_cooldown_secs: u64,

    /// Maximum consecutive failed login attempts before the captcha is
    /// invalidated. Default 5.
    #[serde(default = "default_max_attempts")]
    pub max_failed_attempts: u32,

    /// Length of the generated captcha code. Default 5.
    #[serde(default = "default_captcha_len")]
    pub captcha_len: usize,

    /// Keywords that trigger captcha generation when sent to the official
    /// account by the user. Defaults to ["验证码", "登录码", "login"].
    #[serde(default = "default_trigger_keywords")]
    pub trigger_keywords: Vec<String>,
}

impl Default for WechatAccountConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            account_id: String::new(),
            app_id: String::new(),
            app_secret: String::new(),
            token: String::new(),
            encoding_aes_key: None,
            original_id: None,
            message_mode: "plain".to_string(),
            captcha_ttl_secs: default_captcha_ttl(),
            resend_cooldown_secs: default_resend_cooldown(),
            max_failed_attempts: default_max_attempts(),
            captcha_len: default_captcha_len(),
            trigger_keywords: default_trigger_keywords(),
        }
    }
}

fn default_captcha_ttl() -> u64 {
    300
}
fn default_resend_cooldown() -> u64 {
    60
}
fn default_max_attempts() -> u32 {
    5
}
fn default_captcha_len() -> usize {
    5
}
fn default_trigger_keywords() -> Vec<String> {
    vec![
        "验证码".to_string(),
        "登录码".to_string(),
        "login".to_string(),
    ]
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
                .separator("__")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("server.allowed_origins")
                .with_list_parse_key("database_read_urls")
                .with_list_parse_key("wechat.trigger_keywords"),
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
            allowed_origins: vec!["http://127.0.0.1:3000".to_string()],
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
            database_url: "postgres://127.0.0.1".to_string(),
            redis_url: "redis://127.0.0.1".to_string(),
            cache_max_connections: 10,
            jwt_secret: "secret".to_string(),
            jwt_expiry_seconds: 7200,
            jwt_remember_expiry_seconds: 2592000,
            refresh_token_expiry_seconds: 7776000,
            cookie_secure: true,
            system_admin_email: "admin@webshelf.local".to_string(),
            system_admin_password: "change-me-admin-password".to_string(),
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            database_read_urls: Vec::new(),
            database_routing: DatabaseRoutingConfig::default(),
            database_read: DatabaseReadConfig::default(),
            email: emailserver::EmailConfig::default(),
            wechat: WechatAccountConfig::default(),
        };
        let cloned = config.clone();
        assert_eq!(config.database_url, cloned.database_url);
        assert_eq!(config.jwt_expiry_seconds, cloned.jwt_expiry_seconds);
    }

    /// Verify that `WEBSHELF_SERVER__ALLOWED_ORIGINS` is correctly parsed
    /// as a comma-separated list via `list_separator` + `with_list_parse_key`.
    #[test]
    fn test_allowed_origins_from_env_list() {
        use config::{Config, Environment};
        use std::collections::HashMap;

        let mut source = HashMap::new();
        source.insert(
            "WEBSHELF_SERVER__ALLOWED_ORIGINS".to_string(),
            "https://example.com,https://app.example.com".to_string(),
        );

        let settings = Config::builder()
            .add_source(
                Environment::with_prefix("WEBSHELF")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("server.allowed_origins")
                    .source(Some(source)),
            )
            .build()
            .unwrap();

        let config: AppConfig = settings.try_deserialize().unwrap();
        assert_eq!(config.server.allowed_origins.len(), 2);
        assert_eq!(config.server.allowed_origins[0], "https://example.com");
        assert_eq!(config.server.allowed_origins[1], "https://app.example.com");
    }

    /// Verify that a single-value allowed_origins env var is still parsed as a list.
    #[test]
    fn test_allowed_origins_single_value_from_env() {
        use config::{Config, Environment};
        use std::collections::HashMap;

        let mut source = HashMap::new();
        source.insert(
            "WEBSHELF_SERVER__ALLOWED_ORIGINS".to_string(),
            "https://single.example.com".to_string(),
        );

        let settings = Config::builder()
            .add_source(
                Environment::with_prefix("WEBSHELF")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("server.allowed_origins")
                    .source(Some(source)),
            )
            .build()
            .unwrap();

        let config: AppConfig = settings.try_deserialize().unwrap();
        assert_eq!(config.server.allowed_origins.len(), 1);
        assert_eq!(
            config.server.allowed_origins[0],
            "https://single.example.com"
        );
    }
}
