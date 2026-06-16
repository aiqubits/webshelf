/// Configuration for `RedisRateLimiter`.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Prefix for all Redis keys (e.g. `"ratelimit"` → `ratelimit:login:ip:1.2.3.4`).
    /// Empty string means no prefix.
    pub key_prefix: String,

    /// When `true`, if Redis is unreachable the request is allowed through
    /// (fail‑open). When `false`, the request is rejected with an error.
    pub fail_open: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            key_prefix: "ratelimit".to_string(),
            fail_open: true,
        }
    }
}
