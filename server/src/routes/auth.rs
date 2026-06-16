use axum::{Router, middleware, routing::post};

use crate::AppState;
use crate::handlers::auth::{
    forgot_password, login, register, resend_code, reset_password, verify_email,
};
use crate::middlewares::{RateLimitGuard, rate_limit_middleware};
use distributed_ratelimit::RedisRateLimiter;

/// Helper to wrap a route with a rate‑limiting middleware layer.
fn with_rate_limit(
    route: Router<AppState>,
    limiter: &RedisRateLimiter,
    key_prefix: &'static str,
    ip_max_requests: u64,
    email_max_requests: Option<u64>,
) -> Router<AppState> {
    route.layer(middleware::from_fn_with_state(
        RateLimitGuard {
            limiter: limiter.clone(),
            ip_max_requests,
            ip_window_seconds: 600,
            email_max_requests,
            email_window_seconds: 600,
            key_prefix,
        },
        rate_limit_middleware,
    ))
}

pub fn auth_routes(rate_limiter: RedisRateLimiter) -> Router<AppState> {
    let l = &rate_limiter;

    Router::new()
        .merge(with_rate_limit(
            Router::new().route("/login", post(login)),
            l,
            "login",
            20,
            Some(5),
        ))
        .merge(with_rate_limit(
            Router::new().route("/register", post(register)),
            l,
            "register",
            10,
            None,
        ))
        .merge(with_rate_limit(
            Router::new().route("/verify-email", post(verify_email)),
            l,
            "verify-email",
            20,
            None,
        ))
        .merge(with_rate_limit(
            Router::new().route("/resend-code", post(resend_code)),
            l,
            "resend-code",
            5,
            None,
        ))
        .merge(with_rate_limit(
            Router::new().route("/forgot-password", post(forgot_password)),
            l,
            "forgot-password",
            5,
            None,
        ))
        .merge(with_rate_limit(
            Router::new().route("/reset-password", post(reset_password)),
            l,
            "reset-password",
            10,
            None,
        ))
}
