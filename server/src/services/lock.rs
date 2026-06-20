//! # Distributed Locking
//!
//! Redis-backed distributed locking with safe release (Lua script), retry logic,
//! and automatic lock release on `Drop`.
//!
//! ## 何时需要分布式锁？（刚需场景判断）
//!
//! 分布式锁的适用场景比直觉中要窄得多。在选择分布式锁之前，优先考虑
//! 现有架构中更可靠的替代方案：
//!
//! | 场景 | 首选方案 | 说明 |
//! |---|---|---|
//! | DB 行级并发修改 | `SELECT ... FOR UPDATE` + 事务 | PostgreSQL MVCC 跨副本天然生效 |
//! | 唯一性约束 | DB `UNIQUE` 约束 | 数据库保证原子性，无需锁 |
//! | 分布式 ID 分配 | Snowflake + DB INSERT 自协调 | `worker_id` 通过 UNIQUE 约束分配 |
//! | 原子计数/限流 | Redis `SET NX EX` / `INCR` | Redis 单线程模型保证原子性 |
//! | 条件更新防 TOCTOU | `UPDATE ... WHERE ... < MAX` | 一条 SQL 完成读-改-写 |
//! | 刷新令牌轮转 | 事务内 DELETE + INSERT | 事务保证原子性 |
//!
//! **分布式锁应作为最后手段（last resort）**，仅在以上方案都无法满足时使用。
//!
//! ### 真正需要分布式锁的场景
//!
//! #### 1. 缓存击穿保护（Stampede Protection）
//!
//! K8s 多副本环境下，当热点缓存 key 过期，所有 pod 同时回源查询 DB。
//! 分布式锁让只有一个 pod 回源，其余等待后读缓存。
//! - **正确性**：非必须（没有锁系统仍正确）
//! - **性能**：强烈推荐（防止 DB 负载瞬增 N 倍）
//! - **实现**：[`CacheService::get_or_insert_with_lock`]
//!
//! #### 2. 定时/批处理任务互斥
//!
//! K8s 多副本环境下，如果每个 pod 都独立运行定时任务（如清理过期数据、
//! 批量发送通知），需要分布式锁确保同一时刻只有一个副本执行。
//! - **正确性**：如果任务不可重入/非幂等，则必须
//! - **实现**：[`LockGuard::acquire`] / [`LockGuard::acquire_with_client`]
//!
//! #### 3. 跨副本资源初始化（保护非幂等操作）
//!
//! 某些启动时只需执行一次的操作（如创建外部资源、调用第三方 API），
//! 如果不可重入，需要用分布式锁协调。如果操作已幂等（如数据库迁移），
//! 则无需分布式锁。
//! - **正确性**：如果操作非幂等，则必须
//! - **实现**：[`LockGuard::acquire`]（fail-open，Redis 不可用时跳过关键区）
//!
//! ## 策略选择：fail-open vs fail-close
//!
//! - **fail-open**（[`LockGuard::acquire`]）：Redis 不可用时返回 `None`，
//!   调用方跳过关键区。适用于"有锁更好，没有也能凑合"的场景。
//! - **fail-close**（[`acquire_lock`]）：Redis 不可用时返回 `Err`，
//!   调用方必须处理错误。适用于"没有锁就不能继续"的场景。
//!
//! ## 安全释放
//!
//! 所有锁释放均通过 [`SAFE_RELEASE_SCRIPT`] Lua 脚本完成，原子地检查
//! 锁的值是否匹配，防止误释放其他持有者的锁。锁的默认 TTL 兜底机制
//! 确保即使进程崩溃，锁也不会永久占用。

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

/// Acquire a distributed lock with retry mechanism.
///
/// # 适用场景
///
/// - **需要 fail-close 语义**的场景：当 Redis 不可用时必须让调用方感知错误。
/// - 调用方已通过其他方式确认 Redis 可用（如[`CacheService::is_available`]）。
///
/// # 不使用此函数的场景
///
/// - DB 行级并发 → 使用 `SELECT ... FOR UPDATE` + 事务
/// - 唯一性约束 → 使用 DB `UNIQUE` 约束
/// - 限流器 → 使用 Redis 原子命令（[`distributed-ratelimit`] crate）
///
/// 这是**低层 API**。更推荐使用 [`LockGuard::acquire`]（自动释放锁）。
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

/// Acquire a distributed lock using a pre-established `redis::Client`
/// (from CacheService). Skips the fail-close `Option` check — caller is
/// responsible for ensuring `client` is `Some`.
///
/// # 适用场景
///
/// - 调用方已通过 [`CacheService`] 持有 `redis::Client`，希望复用此实例
///   而非另外维护一个独立的 Redis 客户端。
/// - 需要低层控制（直接获取 `(bool, lock_value)` 元组）。
///
/// # 不使用此函数的场景
///
/// - 需要自动释放锁 → 使用 [`LockGuard::acquire_with_client`]
/// - 不需要精确控制锁值（UUID）→ 使用 [`LockGuard`] 系列 API
///
/// 与 [`acquire_lock`] 不同，本函数接收 `&Client`（而非 `Option<&Client>`），
/// 由调用方负责确保 client 可用。每次调用内部仍然会通过
/// [`redis::Client::get_connection_manager`] 创建新的连接管理器，
/// 这是 redis crate 的 API 限制，无法绕过。
///
/// 本函数的核心价值是重用 CacheService 的 `redis::Client` 实例，
/// 避免调用方自行管理 Redis 客户端配置和生命周期。
pub async fn acquire_lock_with_client(
    client: &Client,
    lock_key: &str,
    expiry_seconds: u64,
    max_retries: u32,
    retry_delay: Duration,
) -> Result<(bool, String)> {
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

/// Release a distributed lock using a pre-established `redis::Client`.
pub async fn release_lock_with_client(
    client: &Client,
    lock_key: &str,
    lock_value: &str,
) -> Result<()> {
    let mut conn = client
        .get_connection_manager()
        .await
        .context("Failed to get async Redis connection")?;

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
    /// # 适用场景
    ///
    /// - **缓存击穿保护**（推荐）：与 [`CacheService::get_or_insert_with_lock`] 配合，
    ///   防止 K8s 多副本同时回源查 DB。
    /// - **定时任务互斥**：多个 K8s pod 同时启动定时任务时，确保只有一个执行。
    /// - **跨副本资源初始化**：只需一个实例完成的启动初始化（已幂等的操作除外）。
    ///
    /// # 不使用此函数的场景
    ///
    /// 大多数并发控制场景已被以下方案覆盖：
    /// - DB 事务 + `FOR UPDATE` → 行级并发修改
    /// - DB `UNIQUE` 约束 → 唯一性保证
    /// - Redis 原子命令 → 计数/限流
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

    /// Acquire a lock using a `redis::Client` from [`CacheService::redis_client`].
    ///
    /// # 适用场景
    ///
    /// 与 [`LockGuard::acquire`] 相同，但：
    /// - 调用方已通过 [`CacheService`] 持有 `redis::Client`，无需重新创建连接
    /// - 适合在已集成缓存的代码路径中复用同一 Redis 连接
    ///
    /// # 与 [`LockGuard::acquire`] 的区别
    ///
    /// - `acquire`：接收 `Option<&Client>`，使用 `acquire_lock`（独立 ConnectionManager）
    /// - `acquire_with_client`：接收 `Option<&Client>`，使用 `acquire_lock_with_client`
    ///   （复用 CacheService 的 redis::Client，减少额外 ConnectionManager 创建开销）
    ///
    /// Both methods are **fail-open**: when the underlying `client` is `None`,
    /// they return `Ok(None)` and the caller proceeds without lock protection.
    /// If fail-close semantics are needed, callers should check
    /// [`CacheService::is_available()`] before calling this method.
    pub async fn acquire_with_client(
        client: Option<&Client>,
        lock_key: &str,
        expiry_seconds: u64,
        max_retries: u32,
        retry_delay: Duration,
    ) -> Result<Option<AcquireResult>> {
        let client = match client {
            Some(c) => c,
            None => {
                tracing::warn!(
                    "Redis client not available, skipping distributed lock for key: {}",
                    lock_key
                );
                return Ok(None);
            }
        };

        let (acquired, lock_value) =
            acquire_lock_with_client(client, lock_key, expiry_seconds, max_retries, retry_delay)
                .await?;

        if acquired {
            Ok(Some(AcquireResult::Acquired(Box::new(Self::new(
                Some(client.clone()),
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
