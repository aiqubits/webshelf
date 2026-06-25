use distributed_ratelimit::RedisRateLimiter;

/// Per-endpoint rate-limit parameters.
///
/// Shared between webshelf-axum and webshelf-salvo adapters so that route
/// definitions (in the server crate) do not need per-framework replicas.
#[derive(Clone)]
pub struct RateLimitGuard {
    pub limiter: RedisRateLimiter,
    pub ip_max_requests: u64,
    pub ip_window_seconds: u64,
    pub email_max_requests: Option<u64>,
    pub email_window_seconds: u64,
    pub key_prefix: &'static str,
}
