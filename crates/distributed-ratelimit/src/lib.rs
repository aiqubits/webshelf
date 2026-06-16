//! # distributed-ratelimit
//!
//! Redis‑backed fixed‑window rate limiter with Tower integration.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use distributed_ratelimit::{RedisRateLimiter, RateLimitConfig};
//!
//! let limiter = RedisRateLimiter::new(redis_client, RateLimitConfig::default());
//!
//! // Check & increment a key
//! let allowed = limiter.check("login:ip:1.2.3.4", 20, 60).await?;
//! ```

mod config;
mod error;
mod limiter;

pub use config::RateLimitConfig;
pub use error::RateLimitError;
pub use limiter::RedisRateLimiter;
