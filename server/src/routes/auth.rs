use crate::AppRouter;
use crate::routes::helpers::{apply_rate_limit, get, post};

use crate::handlers::auth::{
    forgot_password, login, logout, refresh, register, resend_code, reset_password, verify_email,
};
use crate::handlers::wechat::{wechat_enabled, wx_login};
use crate::middlewares::RateLimitGuard;
use distributed_ratelimit::RedisRateLimiter;

/// Build auth routes with rate limiting.
///
/// Window durations are intentionally hardcoded at 600s (10 min) — a single
/// reasonable default that applies uniformly across all auth endpoints.
/// Per-endpoint tuning is done via `ip_max_requests` / `email_max_requests`.
/// If per-endpoint window variation is needed later, promote these to config values.
pub fn auth_routes(rate_limiter: RedisRateLimiter) -> AppRouter {
    let make_guard =
        |key_prefix: &'static str, ip_max_requests: u64, email_max_requests: Option<u64>| {
            RateLimitGuard {
                limiter: rate_limiter.clone(),
                ip_max_requests,
                ip_window_seconds: 600,
                email_max_requests,
                email_window_seconds: 600,
                key_prefix,
            }
        };

    AppRouter::new()
        .merge(apply_rate_limit(
            AppRouter::new().route("/login", post(login)),
            make_guard("login", 20, Some(5)),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/register", post(register)),
            make_guard("register", 10, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/verify-email", post(verify_email)),
            make_guard("verify-email", 20, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/resend-code", post(resend_code)),
            make_guard("resend-code", 5, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/forgot-password", post(forgot_password)),
            make_guard("forgot-password", 5, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/reset-password", post(reset_password)),
            make_guard("reset-password", 10, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/refresh", post(refresh)),
            make_guard("refresh", 30, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/logout", post(logout)),
            make_guard("logout", 30, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/wechat-enabled", get(wechat_enabled)),
            make_guard("wechat-enabled", 60, None),
        ))
        .merge(apply_rate_limit(
            AppRouter::new().route("/wx-login", post(wx_login)),
            make_guard("wx-login", 20, None),
        ))
}
