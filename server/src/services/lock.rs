use anyhow::{Context, Result};
use redis::Client;
use redis::aio::ConnectionManager;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

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

/// SAFE RELEASE Lua script: atomically checks if the lock value matches
/// before deleting. Prevents accidentally releasing another holder's lock.
///
/// KEYS[1] = lock key
/// ARGV[1] = expected lock value (UUID)
/// Returns: 1 if deleted, 0 if value mismatch (lock already released or re-acquired)
const SAFE_RELEASE_SCRIPT: &str = r#"
if redis.call("GET", KEYS[1]) == ARGV[1] then
    return redis.call("DEL", KEYS[1])
else
    return 0
end
"#;

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
/// # Note
/// This is a low-level API that uses a **fail-close** strategy: returns
/// `Err(LockError::RedisNotAvailable)` when Redis is not configured.
/// For a higher-level, fail-open alternative, see [`LockGuard::acquire`].
pub async fn acquire_lock(
    redis_client: Option<&Client>,
    lock_key: &str,
    expiry_seconds: u64,
    max_retries: u32,
    retry_delay: Duration,
) -> Result<(bool, String)> {
    let client = redis_client.ok_or(LockError::RedisNotAvailable)?;

    let conn = client
        .get_connection_manager()
        .await
        .context("Failed to get async Redis connection")?;

    let lock_value = Uuid::new_v4().to_string();
    let acquired = try_acquire_with_retry(
        conn,
        lock_key,
        &lock_value,
        expiry_seconds,
        max_retries,
        retry_delay,
    )
    .await?;

    Ok((acquired, lock_value))
}

async fn try_acquire_with_retry(
    mut conn: ConnectionManager,
    lock_key: &str,
    lock_value: &str,
    expiry_seconds: u64,
    max_retries: u32,
    retry_delay: Duration,
) -> Result<bool> {
    for attempt in 0..max_retries {
        // SET key value NX EX seconds (non-blocking, atomic)
        let result: Option<String> = redis::cmd("SET")
            .arg(lock_key)
            .arg(lock_value)
            .arg("NX")
            .arg("EX")
            .arg(expiry_seconds)
            .query_async(&mut conn)
            .await
            .context("Failed to execute SET NX EX command")?;

        if result.is_some() {
            tracing::debug!(
                "Lock acquired for key: {} on attempt {}",
                lock_key,
                attempt + 1
            );
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

/// Release a distributed lock with ownership verification.
///
/// Uses a Lua script to atomically check that the lock value matches
/// before deleting, preventing accidental release of another holder's lock.
///
/// # Arguments
/// * `redis_client` - Optional Redis client instance
/// * `lock_key` - Unique key for the lock to release
/// * `lock_value` - The UUID value set when the lock was acquired
pub async fn release_lock(
    redis_client: Option<&Client>,
    lock_key: &str,
    lock_value: &str,
) -> Result<()> {
    let client = redis_client.ok_or(LockError::RedisNotAvailable)?;

    let mut conn = client
        .get_connection_manager()
        .await
        .context("Failed to get async Redis connection")?;

    // Use Lua script for atomic GET + compare + DEL
    let script = redis::Script::new(SAFE_RELEASE_SCRIPT);
    let deleted: i32 = script
        .key(lock_key)
        .arg(lock_value)
        .invoke_async(&mut conn)
        .await
        .context("Failed to execute safe release script")?;

    if deleted == 1 {
        tracing::debug!("Lock released for key: {}", lock_key);
    } else {
        tracing::warn!(
            "Lock release skipped for key: {} — value mismatch (lock may have expired or been re-acquired)",
            lock_key
        );
    }

    Ok(())
}

/// Result of a lock acquisition attempt
#[derive(Debug)]
pub enum AcquireResult {
    /// Lock was successfully acquired and guard is returned
    Acquired(Box<LockGuard>),
    /// Lock was not acquired due to contention (another holder has it)
    Contended,
}

/// Lock guard that automatically releases the lock when dropped.
///
/// Stores a unique lock value (UUID) used for safe ownership-verified release.
pub struct LockGuard {
    client: Option<Client>,
    lock_key: String,
    lock_value: String,
}

impl std::fmt::Debug for LockGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LockGuard")
            .field("lock_key", &self.lock_key)
            .field("has_client", &self.client.is_some())
            .finish()
    }
}

impl LockGuard {
    /// Create a new lock guard (internal use)
    fn new(client: Option<Client>, lock_key: String, lock_value: String) -> Self {
        Self {
            client,
            lock_key,
            lock_value,
        }
    }

    /// Acquire a lock and return a guard that releases it on drop.
    ///
    /// # Returns
    /// - `Ok(Some(AcquireResult::Acquired(guard)))` — lock acquired successfully
    /// - `Ok(Some(AcquireResult::Contended))` — lock not acquired due to contention
    /// - `Ok(None)` — Redis not available, lock mechanism is skipped (fail-open)
    ///
    /// # Note
    /// When Redis is not configured (`redis_client` is `None`), this method returns
    /// `Ok(None)` to indicate that distributed locking is unavailable. Callers should
    /// decide whether to proceed without lock protection based on their use case.
    pub async fn acquire(
        redis_client: Option<&Client>,
        lock_key: &str,
        expiry_seconds: u64,
        max_retries: u32,
        retry_delay: Duration,
    ) -> Result<Option<AcquireResult>> {
        if redis_client.is_none() {
            tracing::warn!(
                "Redis not available, skipping distributed lock for key: {}",
                lock_key
            );
            return Ok(None);
        }

        let (acquired, lock_value) = acquire_lock(
            redis_client,
            lock_key,
            expiry_seconds,
            max_retries,
            retry_delay,
        )
        .await?;

        if acquired {
            Ok(Some(AcquireResult::Acquired(Box::new(Self::new(
                redis_client.cloned(),
                lock_key.to_string(),
                lock_value,
            )))))
        } else {
            Ok(Some(AcquireResult::Contended))
        }
    }

    /// Release the lock explicitly (with ownership verification).
    ///
    /// Note: The lock is also released automatically when the guard is dropped.
    /// If this call fails (e.g. transient Redis network error), the fields
    /// are restored to `self` so that `Drop` can retry the release.
    pub async fn release(mut self) -> Result<()> {
        let client = self.client.take();
        let lock_key = std::mem::take(&mut self.lock_key);
        let lock_value = std::mem::take(&mut self.lock_value);
        match release_lock(client.as_ref(), &lock_key, &lock_value).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // Restore fields so Drop can attempt release again
                self.client = client;
                self.lock_key = lock_key;
                self.lock_value = lock_value;
                Err(e)
            }
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            let lock_key = std::mem::take(&mut self.lock_key);
            let lock_value = std::mem::take(&mut self.lock_value);
            if !lock_key.is_empty() {
                // Use Handle::try_current() to avoid panicking when the tokio
                // runtime is unavailable (e.g. during shutdown or on non-tokio threads).
                // In that case the lock will expire naturally via its Redis TTL.
                match tokio::runtime::Handle::try_current() {
                    Ok(handle) => {
                        handle.spawn(async move {
                            if let Err(e) =
                                release_lock(Some(&client), &lock_key, &lock_value).await
                            {
                                tracing::warn!(
                                    "Failed to release lock on drop for key: {}: {}",
                                    lock_key,
                                    e
                                );
                            }
                        });
                    }
                    Err(_) => {
                        tracing::warn!(
                            "No tokio runtime available, lock for key: {} will expire via TTL",
                            lock_key
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running Redis instance
    #[tokio::test]
    #[ignore]
    async fn test_acquire_and_release_lock() {
        let client = Client::open("redis://127.0.0.1:6379").unwrap();
        let lock_key = "test:lock:1";

        // Acquire lock
        let (acquired, lock_value) =
            acquire_lock(Some(&client), lock_key, 10, 1, Duration::from_millis(100))
                .await
                .unwrap();
        assert!(acquired);

        // Release lock (with ownership verification)
        release_lock(Some(&client), lock_key, &lock_value)
            .await
            .unwrap();
    }
}
