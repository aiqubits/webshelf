mod middleware;
mod models;
mod routes;
mod services;
mod utils;

// Re-export for testing
pub use routes::{api_routes, auth_routes};
pub use utils::{init_logger, load_config, AppConfig};

use anyhow::{Context, Result};
use axum::{middleware as axum_middleware, Router};
use clap::Parser;
use http::Method;
use redis::Client as RedisClient;
use sea_orm::{Database, DatabaseConnection};
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::middleware::auth::JwtSecret;

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(name = "webshelf")]
#[command(author, version, about = "The best way to develop your web service with one click.")]
struct Args {
    /// Server bind address
    #[arg(short = 'H', long, default_value = "0.0.0.0")]
    host: String,

    /// Server port
    #[arg(short = 'P', long, default_value_t = 3000)]
    port: u16,

    /// Environment (development, staging, production)
    #[arg(short = 'E', long, default_value = "development")]
    env: String,

    /// Configuration file path
    #[arg(short = 'C', long, default_value = "config.toml")]
    config: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short = 'L', long, default_value = "info")]
    log_level: String,
}

/// Application shared state
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub redis: RedisClient,
    pub config: Arc<AppConfig>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    // Initialize logger
    init_logger(&args.log_level);

    // Setup panic hook for graceful panic handling
    crate::middleware::panic::setup_panic_hook();

    tracing::info!("Starting webshelf in {} mode", args.env);

    // Load configuration
    let app_config = load_config(&args.config, &args.env)
        .context("Failed to load application configuration")?;

    // Use CLI arguments to override config if provided
    let host = if args.host != "0.0.0.0" {
        args.host
    } else {
        app_config.server.host.clone()
    };
    let port = if args.port != 3000 {
        args.port
    } else {
        app_config.server.port
    };

    // Initialize database connection
    tracing::info!("Connecting to database...");
    let db = Database::connect(&app_config.database_url)
        .await
        .context("Failed to connect to database")?;
    tracing::info!("Database connection established");

    // Initialize Redis client
    tracing::info!("Initializing Redis client...");
    let redis_client = RedisClient::open(app_config.redis_url.as_str())
        .context("Failed to create Redis client")?;
    tracing::info!("Redis client initialized");

    // Create shared application state
    let state = AppState {
        db,
        redis: redis_client,
        config: Arc::new(app_config.clone()),
    };

    // Note: Rate limiting can be added with tower-governor when it supports axum 0.7+

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    // Build application router with all middleware
    // Middleware order (innermost to outermost):
    // 1. Panic capture (innermost)
    // 2. Authentication
    // 3. Trace
    // 4. CORS (outermost)
    let app = Router::new()
        // Mount API routes
        .nest("/api", api_routes())
        // Mount auth routes (public)
        .nest("/api/public/auth", auth_routes())
        // Apply middleware stack
        .layer(axum_middleware::from_fn(
            middleware::panic::panic_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state.clone());
    
    // Create a second router to inject JWT secret
    let app_with_jwt = Router::new()
        .nest("/", app)
        .layer(axum::Extension(JwtSecret(app_config.jwt_secret.clone())));

    // Start HTTP server
    let bind_addr = format!("{}:{}", host, port);
    tracing::info!("Starting server on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .context("Failed to bind to address")?;

    tracing::info!("Server is ready to accept connections");

    axum::serve(listener, app_with_jwt)
        .await
        .context("Server failed")?;

    Ok(())
}
