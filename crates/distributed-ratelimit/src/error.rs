use redis::RedisError;

/// Errors that can occur during rate-limit checks.
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    /// Rate limiter has no Redis connection configured.
    #[error("Rate limiter not available (no Redis connection)")]
    NotAvailable,

    /// Redis command failed.
    #[error("Redis error: {0}")]
    Redis(#[from] RedisError),
}
