//! Application bootstrap module.
//!
//! Runtime-specific code (build_app_router) is delegated to submodules:
//! - `axum.rs`: Axum-specific CORS configuration and router construction.
//! - `salvo.rs`: Salvo-specific router construction.
//!
//! Shared bootstrapping logic (config loading, DB init, state creation, server start)
//! lives in this module.

use anyhow::{Context, Result};
use clap::Parser;
use std::sync::Arc;

use crate::{
    AppConfig, AppState, AutoRouter, migrations,
    repositories::user::{Column, Entity as UserEntity},
    routes::helpers::create_rate_limiter,
    utils::{db_router::connect_db, init_logger, load_config},
};
use crate::{AppRouter, AppRuntime, Runtime};

// ── Runtime-specific submodules ──────────────────────────
#[cfg(not(feature = "webshelf-salvo"))]
pub mod axum;
#[cfg(feature = "webshelf-salvo")]
pub mod salvo;

// Re-export build_app_router from the active runtime submodule.
// These convenience wrappers create the rate limiter from the application
// state's cache service and delegate to the submodule.
#[cfg(not(feature = "webshelf-salvo"))]
pub fn build_app_router(state: AppState, env: &str) -> AppRouter {
    let rate_limiter = create_rate_limiter(&state.cache);
    axum::build_app_router(state, env, rate_limiter)
}
#[cfg(feature = "webshelf-salvo")]
pub fn build_app_router(state: AppState, env: &str) -> AppRouter {
    let rate_limiter = create_rate_limiter(&state.cache);
    salvo::build_app_router(state, env, rate_limiter)
}

// Re-export configure_cors (Axum mode only), available for external test usage.
#[cfg(not(feature = "webshelf-salvo"))]
pub use axum::configure_cors;

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
    pub app: AppRouter,
    pub state: AppState,
    pub bind_addr: String,
    pub _worker_handle: crate::snowflake::WorkerHandle,
}

/// Initialize application logger
pub fn init_logging(log_level: &str) {
    init_logger(log_level);
}

/// Setup panic hook for graceful panic handling.
///
/// In salvo mode, the 500 response is handled by the catch_panic middleware
/// — the global panic hook only logs the panic details (identical to axum mode).
pub fn setup_panic_handler() {
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info.payload();
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        tracing::error!(
            target: "panic",
            message = %message,
            location = %location,
            "Application panic occurred"
        );
    }));
    let mode = if cfg!(feature = "webshelf-salvo") {
        "salvo"
    } else {
        "axum"
    };
    tracing::info!("Panic hook installed ({mode} mode: catch_panic middleware returns 500)");
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

/// Create shared application state
pub fn create_app_state(
    db: Arc<AutoRouter>,
    cache: crate::services::CacheService,
    config: AppConfig,
) -> AppState {
    let email_service = emailserver::EmailService::new(config.email.clone());
    AppState {
        db,
        cache,
        config: Arc::new(config),
        email: email_service,
    }
}

/// Bootstrap the entire application
pub async fn bootstrap(cli_args: CliArgs) -> Result<BootstrapResult> {
    tracing::info!("Starting webshelf in {} mode", cli_args.env);

    let app_config = load_app_config(&cli_args)?;

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
        if app_config.system_admin_email.to_lowercase() == "admin@webshelf.local"
            || app_config.system_admin_password == "change-me-admin-password"
        {
            anyhow::bail!(
                "System admin credentials must not use default values in {} environment! \
                 Set WEBSHELF_SYSTEM_ADMIN_EMAIL and WEBSHELF_SYSTEM_ADMIN_PASSWORD environment variables \
                 or update config.toml.",
                cli_args.env
            );
        }
    }

    let host = cli_args
        .host
        .unwrap_or_else(|| app_config.server.host.clone());
    let port = cli_args.port.unwrap_or(app_config.server.port);

    let db = if app_config.database_read_urls.is_empty() {
        let write_db = init_database(&app_config).await?;
        AutoRouter::single(write_db)
    } else {
        let db = AutoRouter::new(
            &app_config.database_url,
            &app_config.database_read_urls,
            &app_config.database,
            &app_config.database_read,
            &app_config.database_routing,
        )
        .await
        .context("Failed to initialize AutoRouter with read replicas")?;

        if app_config.database_routing.health_check_interval_secs > 0 {
            db.clone()
                .start_health_check(std::time::Duration::from_secs(
                    app_config.database_routing.health_check_interval_secs,
                ));
        }
        db
    };

    run_database_migrations(db.write_conn()).await?;

    if let Err(e) = crate::services::auth::cleanup_expired_refresh_tokens(db.write_conn()).await {
        tracing::warn!(
            "Failed to cleanup expired refresh tokens (non-fatal): {:?}",
            e
        );
    }

    let _worker_handle = crate::snowflake::init(db.write_conn()).await?;
    seed_system_admin(db.write_conn(), &app_config).await?;

    let cache =
        crate::services::CacheService::new(&app_config.redis_url, app_config.cache_max_connections)
            .await;

    let state = create_app_state(db, cache, app_config);
    let app = build_app_router(state.clone(), &cli_args.env);

    let bind_addr = format!("{}:{}", host, port);

    Ok(BootstrapResult {
        app,
        state,
        bind_addr,
        _worker_handle,
    })
}

/// Seed system admin account on first boot.
async fn seed_system_admin(db: &sea_orm::DatabaseConnection, config: &AppConfig) -> Result<()> {
    use crate::repositories::user::ActiveModel;
    use crate::utils::password::hash_password;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let email = config.system_admin_email.trim().to_lowercase();

    if email.is_empty() || !validator::ValidateEmail::validate_email(&email) {
        anyhow::bail!(
            "System admin email '{}' is not a valid email address. \
             Set WEBSHELF_SYSTEM_ADMIN_EMAIL to a valid email address (e.g. 'admin@example.com').",
            config.system_admin_email
        );
    }

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
        Ok(_) => tracing::info!("System admin account created: {}", email),
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
        Err(e) => return Err(e).context("Failed to create system admin user"),
    }

    Ok(())
}

/// Start HTTP server with graceful shutdown
pub async fn start_server(bootstrap_result: BootstrapResult) -> Result<()> {
    let BootstrapResult {
        app,
        state,
        bind_addr,
        _worker_handle,
    } = bootstrap_result;
    tracing::info!("Starting server on {}", bind_addr);
    AppRuntime::serve(app, state, &bind_addr).await?;
    tracing::info!("Server shutdown completed");
    Ok(())
}
