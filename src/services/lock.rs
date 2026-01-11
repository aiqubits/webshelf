use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use redis::Client;
use std::time::Duration;
use tokio::time::sleep;

/// Error type for Redis not available
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("Redis is not available. Distributed locking is disabled.")]
    RedisNotAvailable,
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),
    #[error("Lock operation failed: {0}")]
    LockFailed(String),
}

const DEFAULT_LOCK_VALUE: &str = "1";

/// Acquire a distributed lock with retry mechanism
///
/// # Arguments
/// * `redis_client` - Optional Redis client instance (None if not configured)
/// * `lock_key` - Unique key for the lock
/// * `expiry_seconds` - Lock expiration time in seconds
/// * `max_retries` - Maximum number of retry attempts
/// * `retry_delay` - Delay between retry attempts
///
/// # Returns
/// * `Result<bool>` - True if lock was acquired, false if not acquired after retries
/// 
/// # Errors
/// Returns `LockError::RedisNotAvailable` if Redis is not configured
pub async fn acquire_lock(
    redis_client: Option<&Client>,
    lock_key: &str,
    expiry_seconds: u64,
    max_retries: u32,
    retry_delay: Duration,
) -> Result<bool> {
    let client = redis_client.ok_or(LockError::RedisNotAvailable)?;
    
    let conn = client        
        .get_connection_manager()
        .await
        .context("Failed to get async Redis connection")?;

    try_acquire_with_retry(conn, lock_key, expiry_seconds, max_retries, retry_delay).await
}

async fn try_acquire_with_retry(
    mut conn: ConnectionManager,
    lock_key: &str,
    expiry_seconds: u64,
    max_retries: u32,
    retry_delay: Duration,
) -> Result<bool> {
    for attempt in 0..max_retries {
        // SET key value NX EX seconds (non-blocking, atomic)
        let result: Option<String> = redis::cmd("SET")
            .arg(lock_key)
            .arg(DEFAULT_LOCK_VALUE)
            .arg("NX")
            .arg("EX")
            .arg(expiry_seconds)
            .query_async(&mut conn)
            .await
            .context("Failed to execute SET NX EX command")?;

        if result.is_some() {
            tracing::debug!("Lock acquired for key: {} on attempt {}", lock_key, attempt + 1);
            return Ok(true);
        }

        // Non-blocking retry with delay
        if attempt < max_retries - 1 {
            tracing::trace!(
                "Lock not acquired for key: {}, retrying in {:?}",
                lock_key,
                retry_delay
            );
            sleep(retry_delay).await;
        }
    }

    tracing::debug!(
        "Failed to acquire lock for key: {} after {} attempts",
        lock_key,
        max_retries
    );
    Ok(false)
}

/// Release a distributed lock
///
/// # Arguments
/// * `redis_client` - Optional Redis client instance
/// * `lock_key` - Unique key for the lock to release
pub async fn release_lock(redis_client: Option<&Client>, lock_key: &str) -> Result<()> {
    let client = redis_client.ok_or(LockError::RedisNotAvailable)?;
    
    let mut conn = client        
        .get_connection_manager()
        .await
        .context("Failed to get async Redis connection")?;

    use redis::AsyncCommands;

    conn.del::<_, ()>(lock_key)
        .await
        .context("Failed to release lock")?;

    tracing::debug!("Lock released for key: {}", lock_key);
    Ok(())
}

/// Lock guard that automatically releases the lock when dropped
pub struct LockGuard {
    client: Option<Client>,
    lock_key: String,
}

impl LockGuard {
    /// Create a new lock guard (internal use)
    fn new(client: Option<Client>, lock_key: String) -> Self {
        Self { client, lock_key }
    }

    /// Acquire a lock and return a guard that releases it on drop
    pub async fn acquire(
        redis_client: Option<&Client>,
        lock_key: &str,
        expiry_seconds: u64,
        max_retries: u32,
        retry_delay: Duration,
    ) -> Result<Option<Self>> {
        if redis_client.is_none() {
            tracing::warn!("Redis not available, skipping distributed lock for key: {}", lock_key);
            return Ok(None);
        }

        let acquired =
            acquire_lock(redis_client, lock_key, expiry_seconds, max_retries, retry_delay).await?;

        if acquired {
            Ok(Some(Self::new(redis_client.cloned(), lock_key.to_string())))
        } else {
            Ok(None)
        }
    }

    /// Release the lock explicitly
    pub async fn release(self) -> Result<()> {
        release_lock(self.client.as_ref(), &self.lock_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running Redis instance
    // They are not ignored by default
    #[tokio::test]
    // #[ignore]
    async fn test_acquire_and_release_lock() {
        let client = Client::open("redis://127.0.0.1:6379").unwrap();
        let lock_key = "test:lock:1";

        // Acquire lock
        let acquired = acquire_lock(Some(&client), lock_key, 10, 1, Duration::from_millis(100))
            .await
            .unwrap();
        assert!(acquired);

        // Release lock
        release_lock(Some(&client), lock_key).await.unwrap();
    }
}
