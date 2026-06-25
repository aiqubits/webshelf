//! Axum-specific bootstrap: CORS configuration and router construction.

use crate::middlewares::{auth_middleware, panic_middleware};
use crate::routes::{api_routes, auth_routes};
use crate::{AppRouter, AppState};
use distributed_ratelimit::RedisRateLimiter;
use webshelf_axum::{
    Any, CompressionLayer, CorsLayer, HeaderValue, Method, RequestBodyLimitLayer, TraceLayer,
    from_fn, from_fn_with_state,
};

/// Configure CORS layer (Axum mode)
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
            // No .allow_origin() — CorsLayer defaults to denying all origins.
            // This is intentional: when behind a reverse proxy (nginx) that
            // handles cross-origin, the application-layer CORS should be locked
            // down. If direct cross-origin access is needed, configure
            // server.allowed_origins in config.toml.
            return CorsLayer::new()
                .allow_methods([Method::OPTIONS])
                .allow_headers(Any);
        }
        return use_any();
    }

    let mut origins: Vec<HeaderValue> = Vec::with_capacity(allowed_origins.len());
    for origin in allowed_origins {
        match origin.parse::<HeaderValue>() {
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
        if env != "development" {
            tracing::warn!(
                "CORS: all configured allowed_origins failed to parse, returning restrictive CORS"
            );
            // Same reasoning as above — omit .allow_origin() to deny all origins.
            return CorsLayer::new()
                .allow_methods([Method::OPTIONS])
                .allow_headers(Any);
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

/// Build application router — Axum version
///
/// `rate_limiter` is accepted as a parameter so that test helpers can inject
/// a disabled limiter without duplicating the middleware chain.
/// Production code should call the convenience wrapper in `bootstrap::mod.rs`.
pub fn build_app_router(state: AppState, env: &str, rate_limiter: RedisRateLimiter) -> AppRouter {
    let allowed_origins = state.config.server.allowed_origins.clone();
    let cors = configure_cors(&allowed_origins, env);
    let compression = CompressionLayer::new();

    AppRouter::new()
        .nest(
            "/api",
            api_routes().layer(from_fn_with_state(
                state.clone(),
                auth_middleware::<AppState>,
            )),
        )
        .nest("/api/public/auth", auth_routes(rate_limiter))
        .layer(from_fn(panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(compression)
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
}
