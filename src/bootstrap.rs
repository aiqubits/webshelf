use anyhow::{Context, Result};
use axum::{middleware as axum_middleware, Router};
use clap::Parser;
use http::Method;
use redis::Client as RedisClient;
use sea_orm::Database;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
    compression::CompressionLayer,
};

use crate::{
    middleware::{panic, auth::JwtSecret},
    migrations,
    routes::{api_routes, auth_routes},
    utils::{init_logger, load_config},
    AppConfig, AppState,
};

/// Command-line arguments
#[derive(Parser, Debug, Clone)]
#[command(name = "webshelf")]
#[command(author, version, about = "The best way to develop your web service with one click.")]
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
pub async fn init_database(config: &AppConfig) -> Result<sea_orm::DatabaseConnection> {
    tracing::info!("Connecting to database...");
    let db = Database::connect(&config.database_url)
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

    let redis_client = match RedisClient::open(config.redis_url.as_str()) {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("Failed to create Redis client: {}. System will run without distributed locking support.", e);
            return Ok(None);
        }
    };

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
    db: sea_orm::DatabaseConnection,
    redis: Option<RedisClient>,
    config: AppConfig,
) -> AppState {
    AppState {
        db,
        redis,
        config: Arc::new(config),
    }
}

/// Configure CORS layer
pub fn configure_cors() -> CorsLayer {
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
}

/// Build application router with all middleware
pub fn build_app_router(state: AppState, jwt_secret: String) -> Router {
    let cors = configure_cors();
    let compression = CompressionLayer::new();

    Router::new()
        .nest("/api", api_routes())
        .nest("/api/public/auth", auth_routes())
        .layer(axum_middleware::from_fn(panic::panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(compression)
        .layer(axum::Extension(JwtSecret(jwt_secret)))
        .with_state(state)
}

/// Bootstrap the entire application
pub async fn bootstrap(cli_args: CliArgs) -> Result<BootstrapResult> {
    tracing::info!("Starting webshelf in {} mode", cli_args.env);

    let app_config = load_app_config(&cli_args)?;

    let host = cli_args.host.unwrap_or_else(|| app_config.server.host.clone());
    let port = cli_args.port.unwrap_or_else(|| app_config.server.port);

    let db = init_database(&app_config).await?;
    run_database_migrations(&db).await?;

    let redis_client = init_redis(&app_config).await?;

    let state = create_app_state(db, redis_client, app_config.clone());
    let app = build_app_router(state, app_config.jwt_secret.clone());

    let bind_addr = format!("{}:{}", host, port);

    Ok(BootstrapResult { app, bind_addr })
}

/// Start HTTP server with graceful shutdown
pub async fn start_server(bootstrap_result: BootstrapResult) -> Result<()> {
    let BootstrapResult { app, bind_addr } = bootstrap_result;

    tracing::info!("Starting server on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .context("Failed to bind to address")?;

    tracing::info!("Server is ready to accept connections");

    axum::serve(listener, app)
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
