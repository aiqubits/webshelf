use anyhow::{Context, Result};
use axum::{Router, middleware as axum_middleware};
use clap::Parser;
use http::Method;
use redis::Client as RedisClient;
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};

use crate::{
    AppConfig, AppState, AutoRouter,
    middlewares::{
        auth::auth_middleware,
        panic::{self, panic_middleware},
    },
    migrations,
    repositories::user::{Column, Entity as UserEntity},
    routes::{api_routes, auth_routes},
    utils::{db_router::connect_db, init_logger, load_config},
};
use distributed_ratelimit::{RateLimitConfig, RedisRateLimiter};

/// Command-line arguments
#[derive(Parser, Debug, Clone)]
#[command(name = "webshelf")]
#[command(
    author,
    version,
    about = "The best way to develop your web service with one click."
)]
pub struct CliArgs {
    /// Server bind address (overrides config file)
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Server port (overrides config file)
    #[arg(short = 'P', long)]
    pub port: Option<u16>,

    /// Environment (development, staging, production)
    #[arg(short = 'E', long, default_value = "development")]
    pub env: String,

    /// Configuration file path
    #[arg(short = 'C', long, default_value = "config.toml")]
    pub config: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short = 'L', long, default_value = "info")]
    pub log_level: String,
}

/// Application bootstrap result containing all initialized components
pub struct BootstrapResult {
    pub app: Router,
    pub bind_addr: String,
    pub _worker_handle: crate::snowflake::WorkerHandle,
}

/// Initialize application logger
pub fn init_logging(log_level: &str) {
    init_logger(log_level);
}

/// Setup panic hook for graceful panic handling
pub fn setup_panic_handler() {
    panic::setup_panic_hook();
}

/// Load and merge configuration from file and CLI arguments
pub fn load_app_config(cli_args: &CliArgs) -> Result<AppConfig> {
    let app_config = load_config(&cli_args.config, &cli_args.env)
        .context("Failed to load application configuration")?;
    Ok(app_config)
}

/// Initialize database connection
///
/// Delegates to `db_router::connect_db` for the actual connection setup.
pub async fn init_database(config: &AppConfig) -> Result<sea_orm::DatabaseConnection> {
    tracing::info!("Connecting to database...");
    let db = connect_db(&config.database_url, &config.database)
        .await
        .context("Failed to connect to database")?;
    tracing::info!("Database connection established");
    Ok(db)
}

/// Run database migrations
pub async fn run_database_migrations(db: &sea_orm::DatabaseConnection) -> Result<()> {
    tracing::info!("Running database migrations...");
    migrations::run_migrations(db)
        .await
        .context("Failed to run migrations")?;
    tracing::info!("Migrations completed");
    Ok(())
}

/// Initialize and verify Redis client
pub async fn init_redis(config: &AppConfig) -> Result<Option<RedisClient>> {
    tracing::info!("Initializing Redis client...");
    if config.redis_url.is_empty() {
        tracing::warn!("Redis URL is empty. System will run without distributed locking support.");
        return Ok(None);
    }

    let redis_client = RedisClient::open(config.redis_url.as_str())
        .context("Failed to create Redis client from configured redis_url")?;

    let mut redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .context("Failed to establish Redis connection")?;

    let pong: String = redis::cmd("PING")
        .query_async(&mut redis_conn)
        .await
        .context("Redis PING failed")?;

    tracing::info!("Redis client initialized and connection verified: {}", pong);
    Ok(Some(redis_client))
}

/// Create shared application state
pub fn create_app_state(
    db: Arc<AutoRouter>,
    redis: Option<RedisClient>,
    config: AppConfig,
) -> AppState {
    let email_service = emailserver::EmailService::new(config.email.clone());
    AppState {
        db,
        redis,
        config: Arc::new(config),
        email: email_service,
    }
}

/// Configure CORS layer
///
/// Uses allowed_origins from config if specified. In non-development environments
/// with no allowed_origins, logs an error and returns a restrictive CORS layer that
/// effectively blocks all cross-origin requests (only OPTIONS preflight is allowed).
/// In development, falls back to `Any` with a warning log.
pub fn configure_cors(allowed_origins: &[String], env: &str) -> CorsLayer {
    let use_any = || {
        tracing::warn!(
            "CORS: using Any (allow all origins). \
             This is acceptable for development but NOT recommended for production."
        );
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
                Method::OPTIONS,
            ])
            .allow_headers(Any)
    };

    if allowed_origins.is_empty() {
        if env != "development" {
            tracing::warn!(
                "CORS: no allowed_origins configured in {} environment. \
                 If using a reverse proxy (nginx) for same-origin serving, this is expected. \
                 Otherwise, set server.allowed_origins in config.toml or via WEBSHELF_SERVER__ALLOWED_ORIGINS.",
                env
            );
            // Return a restrictive CORS layer that effectively denies all cross-origin requests.
            // An empty allow_origin list means no origin matches; OPTIONS is allowed only
            // so preflight requests return a response instead of hanging.
            //
            // NOTE: allow_headers(Any) here is safe because no origin is allowed —
            // it only ensures the preflight response includes the correct
            // Access-Control-Allow-Headers header for debugging convenience.
            return CorsLayer::new()
                .allow_methods([Method::OPTIONS])
                .allow_headers(tower_http::cors::Any);
        }
        return use_any();
    }

    let mut origins: Vec<http::HeaderValue> = Vec::with_capacity(allowed_origins.len());
    for origin in allowed_origins {
        match origin.parse::<http::HeaderValue>() {
            Ok(header_value) => origins.push(header_value),
            Err(e) => {
                tracing::error!(
                    "CORS: failed to parse allowed_origin '{}': {}. \
                     This origin will be ignored — check your config.toml or WEBSHELF_SERVER__ALLOWED_ORIGINS.",
                    origin,
                    e
                );
            }
        }
    }

    if origins.is_empty() {
        // In non-development environments, a misconfigured (all-invalid) origin list
        // is treated the same as an empty list: return a restrictive CORS layer.
        // This prevents accidentally opening up CORS to all origins due to a typo.
        if env != "development" {
            tracing::warn!(
                "CORS: all configured allowed_origins failed to parse, returning restrictive CORS"
            );
            return CorsLayer::new()
                .allow_methods([Method::OPTIONS])
                .allow_headers(tower_http::cors::Any);
        }
        tracing::warn!(
            "CORS: all configured allowed_origins failed to parse: {:?}, falling back to Any (development only)",
            allowed_origins
        );
        return use_any();
    }

    tracing::info!("CORS: allowing origins: {:?}", origins);
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

/// Build application router with all middleware
pub fn build_app_router(state: AppState, env: &str) -> Router {
    let allowed_origins = state.config.server.allowed_origins.clone();
    let cors = configure_cors(&allowed_origins, env);
    let compression = CompressionLayer::new();

    // Create the login rate limiter (disabled if Redis is not configured).
    let rate_limiter = match &state.redis {
        Some(client) => RedisRateLimiter::new(client.clone(), RateLimitConfig::default()),
        None => {
            tracing::warn!(
                "Redis not available — login rate limiting is disabled. \
                 Set WEBSHELF_REDIS_URL or redis_url in config.toml to enable."
            );
            RedisRateLimiter::disabled(RateLimitConfig::default())
        }
    };

    // Middleware layers are applied in reverse order (last added = first to execute).
    Router::new()
        .nest("/api", api_routes())
        .nest("/api/public/auth", auth_routes(rate_limiter))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(axum_middleware::from_fn(panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(compression)
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)) // 10MB max request body
        .with_state(state)
}

/// Bootstrap the entire application
pub async fn bootstrap(cli_args: CliArgs) -> Result<BootstrapResult> {
    tracing::info!("Starting webshelf in {} mode", cli_args.env);

    let app_config = load_app_config(&cli_args)?;

    // Reject default or weak JWT secret in non-development environments
    if cli_args.env != "development" {
        let is_default = app_config.jwt_secret == "REPLACE_ME_WITH_A_STRONG_SECRET";
        if is_default {
            anyhow::bail!(
                "JWT secret must be changed from the default value in {} environment! \
                 Set WEBSHELF_JWT_SECRET environment variable or update config.toml.",
                cli_args.env
            );
        }
        if app_config.jwt_secret.len() < 32 {
            anyhow::bail!(
                "JWT secret must be at least 32 characters long in {} environment (current: {}). \
                 Generate a strong secret with: openssl rand -base64 64",
                cli_args.env,
                app_config.jwt_secret.len()
            );
        }

        // Reject default system admin credentials in non-development environments.
        // Default credentials are a critical security risk — they must be changed
        // before the application can start in production or staging.
        if app_config.system_admin_email.to_lowercase() == "admin@webshelf.local"
            || app_config.system_admin_password == "change-me-admin-password"
        {
            anyhow::bail!(
                "System admin credentials must not use default values in {} environment! \
                 Set WEBSHELF_SYSTEM_ADMIN_EMAIL and WEBSHELF_SYSTEM_ADMIN_PASSWORD environment variables \
                 or update config.toml. Default credentials will be rejected on every startup.",
                cli_args.env
            );
        }
    }

    let host = cli_args
        .host
        .unwrap_or_else(|| app_config.server.host.clone());
    let port = cli_args.port.unwrap_or(app_config.server.port);

    let db = if app_config.database_read_urls.is_empty() {
        // No read replicas configured — single-database mode (backward compatible)
        let write_db = init_database(&app_config).await?;
        AutoRouter::single(write_db)
    } else {
        // Read replicas configured — read-write split mode
        let db = AutoRouter::new(
            &app_config.database_url,
            &app_config.database_read_urls,
            &app_config.database,
            &app_config.database_read,
            &app_config.database_routing,
        )
        .await
        .context("Failed to initialize AutoRouter with read replicas")?;

        // Start background health check if configured
        if app_config.database_routing.health_check_interval_secs > 0 {
            db.clone()
                .start_health_check(std::time::Duration::from_secs(
                    app_config.database_routing.health_check_interval_secs,
                ));
        }

        db
    };

    // Migrations must run on the write database
    run_database_migrations(db.write_conn()).await?;

    // Clean up expired refresh tokens (write database only)
    if let Err(e) = crate::services::auth::cleanup_expired_refresh_tokens(db.write_conn()).await {
        tracing::warn!(
            "Failed to cleanup expired refresh tokens (non-fatal): {:?}",
            e
        );
    }

    // Initialize Snowflake ID generator (must happen before seed_system_admin)
    let _worker_handle = crate::snowflake::init(db.write_conn()).await?;

    seed_system_admin(db.write_conn(), &app_config).await?;

    let redis_client = init_redis(&app_config).await?;

    let state = create_app_state(db, redis_client, app_config);
    let app = build_app_router(state, &cli_args.env);

    let bind_addr = format!("{}:{}", host, port);

    Ok(BootstrapResult {
        app,
        bind_addr,
        _worker_handle,
    })
}

/// Seed system admin account on first boot.
///
/// Checks if a user with the configured `system_admin_email` already exists.
/// If not, creates a new user with role "system". The system account is the
/// super-admin: only one can exist, and it bypasses all admin-only restrictions.
async fn seed_system_admin(db: &sea_orm::DatabaseConnection, config: &AppConfig) -> Result<()> {
    use crate::repositories::user::ActiveModel;
    use crate::utils::password::hash_password;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let email = config.system_admin_email.trim().to_lowercase();

    // Fail fast on a misconfigured system admin email rather than silently
    // creating a malformed account. Email is a critical identifier for the
    // super-admin and must be valid for login and password recovery flows.
    if email.is_empty() || !validator::ValidateEmail::validate_email(&email) {
        anyhow::bail!(
            "System admin email '{}' is not a valid email address. \
             Set WEBSHELF_SYSTEM_ADMIN_EMAIL to a valid email address \
             (e.g. 'admin@example.com').",
            config.system_admin_email
        );
    }

    // Check if system admin already exists
    let existing = UserEntity::find()
        .filter(Column::Email.eq(&email))
        .one(db)
        .await
        .context("Failed to query system admin user")?;

    if let Some(user) = existing {
        if user.role == "system" {
            tracing::info!("System admin account already exists: {}", email);
            return Ok(());
        }
        anyhow::bail!(
            "A non-system user already exists with the configured system admin email '{}'. \
             This is a data integrity issue — a regular user must not occupy the system admin email. \
             Please change WEBSHELF_SYSTEM_ADMIN_EMAIL or remove the conflicting user.",
            email
        );
    }

    // Create system admin user
    let password_hash = hash_password(&config.system_admin_password)
        .context("Failed to hash system admin password")?;

    let now = chrono::Utc::now();
    let user = ActiveModel {
        id: Set(crate::snowflake::generate_id()),
        email: Set(email.clone()),
        password_hash: Set(password_hash),
        name: Set("System Administrator".to_string()),
        role: Set("system".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        token_version: Set(1),
        email_verified: Set(true),
        verification_code_hash: Set(None),
        verification_code_expires_at: Set(None),
        verification_code_sent_at: Set(None),
        verification_failed_attempts: Set(0),
        password_reset_token_hash: Set(None),
        password_reset_expires_at: Set(None),
        password_reset_sent_at: Set(None),
        password_reset_failed_attempts: Set(0),
        balance: Set(0),
    };

    match user.insert(db).await {
        Ok(_) => {
            tracing::info!("System admin account created: {}", email);
        }
        Err(e)
            if matches!(
                e.sql_err(),
                Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
            ) =>
        {
            tracing::info!(
                "System admin account already exists (race condition handled): {}",
                email
            );
        }
        Err(e) => {
            return Err(e).context("Failed to create system admin user");
        }
    }

    Ok(())
}

/// Start HTTP server with graceful shutdown
pub async fn start_server(bootstrap_result: BootstrapResult) -> Result<()> {
    let BootstrapResult {
        app,
        bind_addr,
        _worker_handle,
    } = bootstrap_result;

    tracing::info!("Starting server on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .context("Failed to bind to address")?;

    tracing::info!("Server is ready to accept connections");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("Server failed")?;

    tracing::info!("Server shutdown completed");
    Ok(())
}

/// Wait for shutdown signal (SIGTERM or SIGINT)
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        tracing::info!("Received Ctrl+C signal, initiating graceful shutdown");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
        tracing::info!("Received SIGTERM signal, initiating graceful shutdown");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
