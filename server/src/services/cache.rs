//! # Cache Service
//!
//! Unified Redis caching layer with bb8 connection pooling.
//!
//! ## Design
//!
//! - Uses `bb8` + `bb8-redis` for connection pooling.
//! - Graceful degradation: when Redis is unavailable, all operations silently
//!   no-op. The application never fails to start or crash due to cache errors.
//! - `get_or_insert` pattern: auto-populates cache on miss, with optional
//!   negative caching via `set_null` to prevent cache penetration.
//! - `connection_manager()` accessor allows `distributed-ratelimit` and the
//!   lock service to **share the same connection pool**, eliminating the
//!   previous fragmentation of three independent Redis connection lifecycles.
//!
//! ## Graceful Degradation
//!
//! Unlike the original `init_redis` (which uses `?` and prevents server startup
//! on Redis failure), `CacheService::new` catches all Redis connection errors,
//! logs a warning, and continues with `conn: None`. All public methods on a
//! `None` connection immediately return `Ok(None)` or `Ok(())`.
use std::time::Duration;

use bb8::Pool;
use bb8_redis::RedisConnectionManager;
use serde::{Serialize, de::DeserializeOwned};

use crate::services::lock::AcquireResult;

/// Redis connection pool type alias.
type RedisPool = Pool<RedisConnectionManager>;

/// Cache service error.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Redis pool error: {0}")]
    Pool(#[from] bb8::RunError<bb8_redis::redis::RedisError>),

    #[error("Redis command error: {0}")]
    Redis(#[from] bb8_redis::redis::RedisError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Cache not available (Redis not configured or unreachable)")]
    NotAvailable,

    #[error("Fallback computation failed: {0}")]
    FallbackFailed(String),
}

pub type CacheResult<T> = std::result::Result<T, CacheError>;

/// Unified cache service, shared across the application via `AppState.cache`.
///
/// When Redis is unavailable (`conn` is `None`), all operations silently
/// no-op — the service never returns errors from `get()`, `set()`, `invalidate()`
/// when the pool is absent.
#[derive(Clone)]
pub struct CacheService {
    pool: Option<RedisPool>,
    /// A `redis::Client` for use by sibling services (rate limiter, lock)
    /// that need their own `ConnectionManager`. This avoids forcing them to
    /// depend on bb8 types.
    client: Option<redis::Client>,
    /// Max number of concurrent connections the pool can open.
    max_size: u32,
}

impl std::fmt::Debug for CacheService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheService")
            .field("pool", &self.pool.as_ref().map(|_| "<bb8 pool>"))
            .field("client", &self.client.as_ref().map(|_| "<redis client>"))
            .field("max_size", &self.max_size)
            .finish()
    }
}

impl CacheService {
    /// Create a new cache service with a bb8 connection pool.
    ///
    /// Also creates a `redis::Client` so that sibling services (rate limiter,
    /// lock service) can share the same Redis endpoint without creating their
    /// own connections.
    ///
    /// # Graceful degradation
    ///
    /// If `redis_url` is empty or unreachable, the service logs a warning and
    /// runs in **no-op mode** — all operations silently succeed without doing
    /// any Redis work. The server never fails to start due to a Redis issue.
    pub async fn new(redis_url: &str, max_size: u32) -> Self {
        if redis_url.is_empty() {
            tracing::info!(
                "CacheService: Redis URL is empty, running in no-op mode. \
                 Set WEBSHELF_REDIS_URL or redis_url in config.toml to enable caching."
            );
            return Self {
                pool: None,
                client: None,
                max_size,
            };
        }

        // Create redis::Client for sharing with rate limiter / lock service.
        // This uses the project's `redis` 1.x crate (not bb8-redis's internal redis).
        let client = redis::Client::open(redis_url).ok();
        if client.is_none() {
            tracing::warn!(
                "CacheService: Failed to parse redis URL: '{}'. Running in no-op mode.",
                redis_url
            );
        }

        let manager = match RedisConnectionManager::new(redis_url) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    "CacheService: Failed to create RedisConnectionManager: {:?}. \
                     Running in no-op mode.",
                    e
                );
                return Self {
                    pool: None,
                    client,
                    max_size,
                };
            }
        };

        let pool = match Pool::builder().max_size(max_size).build(manager).await {
            Ok(p) => {
                tracing::info!(
                    "CacheService: bb8 pool established (max_size={}).",
                    max_size
                );
                p
            }
            Err(e) => {
                tracing::warn!(
                    "CacheService: Failed to build bb8 pool: {:?}. \
                     Running in no-op mode (client still available for rate limiter).",
                    e
                );
                return Self {
                    pool: None,
                    client,
                    max_size,
                };
            }
        };

        Self {
            pool: Some(pool),
            client,
            max_size,
        }
    }

    // ── Public API ───────────────────────────────────────────────────────

    /// Returns `true` if a Redis pool is available.
    pub fn is_available(&self) -> bool {
        self.pool.is_some()
    }

    /// Return a reference to the inner `redis::Client` for sharing with
    /// the rate limiter and lock service.
    ///
    /// These services already know how to use `redis::Client` to create their
    /// own `ConnectionManager` — this avoids forcing them to depend on bb8 types.
    pub fn redis_client(&self) -> Option<&redis::Client> {
        self.client.as_ref()
    }

    // ── get / get_or_insert ──────────────────────────────────────────────

    /// Retrieve and deserialize a cached value.
    ///
    /// Returns `Ok(None)` when:
    /// - The key does not exist in Redis.
    /// - Redis is not available (no-op mode).
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> CacheResult<Option<T>> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(None),
        };

        let raw: Option<String> = bb8_redis::redis::cmd("GET")
            .arg(key)
            .query_async(&mut *conn)
            .await?;

        match raw {
            Some(s) => {
                let val: T = serde_json::from_str(&s)
                    .map_err(|e| CacheError::Deserialization(e.to_string()))?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    /// Retrieve from cache, or compute-and-store on miss.
    ///
    /// On cache miss, runs `f` to compute the value, stores it with `ttl`,
    /// and returns it.  The cache is checked BEFORE the fallback, and a
    /// pre-existing negative-cache marker (set via [`Self::set_null`]) causes
    /// an immediate [`CacheError::FallbackFailed`] without running `f`.
    ///
    /// # Negative cache
    ///
    /// This method does **not** automatically store a negative-cache marker
    /// when `f` fails — it cannot distinguish "entity does not exist" from
    /// "temporary error".  If you need cache-penetration protection, call
    /// [`Self::set_null`] explicitly in your fallback path (see `get_user`
    /// in `user.rs` for an example).
    ///
    /// # Errors
    ///
    /// Returns `CacheError::FallbackFailed` when `f` returns an error.
    pub async fn get_or_insert<T, F, E>(
        &self,
        key: &str,
        ttl: Duration,
        f: F,
    ) -> Result<T, CacheError>
    where
        T: Serialize + DeserializeOwned + Send,
        F: std::future::Future<Output = Result<T, E>> + Send,
        E: std::fmt::Display,
    {
        // 1. Try cache hit
        if self.is_available() {
            if let Some(val) = self.get::<T>(key).await? {
                return Ok(val);
            }
            // 2. Check negative-cache marker (entity does not exist)
            if self.exists(&format!("{}:null", key)).await? {
                return Err(CacheError::FallbackFailed(
                    "Entity does not exist (negative cache)".to_string(),
                ));
            }
        }

        // 3. Fallback computation
        let val = f
            .await
            .map_err(|e| CacheError::FallbackFailed(format!("Fallback failed: {}", e)))?;

        // 4. Populate cache (best-effort — failures are logged, not propagated)
        if let Err(e) = self.set(key, &val, ttl).await {
            tracing::warn!("Cache set failed for key '{}': {:?}", key, e);
        }
        Ok(val)
    }

    /// Cache-stampede-protected variant of [`get_or_insert`](Self::get_or_insert).
    ///
    /// # 适用场景：热点缓存 key 击穿保护
    ///
    /// 在 K8s 多副本部署下，当高频访问的缓存 key 过期时，所有 pod 可能
    /// 同时回源查询 DB，导致负载瞬增。本方法使用分布式锁确保仅一个 pod
    /// 回源计算，其他 pod 等待后从缓存读取。
    ///
    /// 典型的适用场景：
    /// - 全局共享配置（功能开关、定价表）
    /// - 高并发公共统计数据
    /// - 启动时多 pod 同时热数据回填
    ///
    /// # 不使用此方法的场景
    ///
    /// - **普通缓存**：使用 [`get_or_insert`](Self::get_or_insert) 即可，
    ///   不加锁的并发回源对大多数 key 是可接受的。
    /// - **用户私有数据**：每个用户 key 的并发度极低（一个人同时登录多个
    ///   设备的情况），无需锁保护。
    /// - **DB 事务性操作**：使用 `SELECT ... FOR UPDATE` 行级锁。
    ///
    /// # 何时不值得用
    ///
    /// 如果回源计算极快（<1ms DB 查询），加锁的额外开销可能超过收益。
    /// 对这类 key，主动预热（高峰期前调用 [`set`](Self::set)）效果更好。
    ///
    /// Uses a distributed lock (via `LockGuard`) to ensure only one concurrent
    /// request recomputes the value on cache miss. Other concurrent requests
    /// that fail to acquire the lock will **wait and retry** — they poll the
    /// cache in a short loop until the winning request populates it.
    ///
    /// Use this for **extremely hot keys** where many concurrent misses could
    /// overload the database (e.g., globally shared configuration, highly
    /// contended stats).
    ///
    /// # Requirements
    ///
    /// This method requires CacheService to have a `redis::Client` (for locking).
    /// If Redis is not available, it falls back to the lock-free `get_or_insert`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user = cache
    ///     .get_or_insert_with_lock(
    ///         "user:profile:123",
    ///         Duration::from_secs(300),
    ///         Duration::from_secs(5),   // lock expiry
    ///         Duration::from_millis(50), // retry delay
    ///         5,                         // max retries
    ///         async { db::find_user(123).await },
    ///     )
    ///     .await?;
    /// ```
    pub async fn get_or_insert_with_lock<T, F, E>(
        &self,
        key: &str,
        ttl: Duration,
        lock_expiry_secs: u64,
        retry_delay: Duration,
        max_retries: u32,
        f: F,
    ) -> Result<T, CacheError>
    where
        T: Serialize + DeserializeOwned + Send,
        F: std::future::Future<Output = Result<T, E>> + Send,
        E: std::fmt::Display,
    {
        // 1. Fast path: try cache first
        if self.is_available()
            && let Some(val) = self.get::<T>(key).await?
        {
            return Ok(val);
        }

        // 1b. Check negative-cache marker (entity does not exist)
        //     Prevents repeated lock acquisition for non-existent keys.
        if self.is_available() && self.exists(&format!("{}:null", key)).await? {
            return Err(CacheError::FallbackFailed(
                "Entity does not exist (negative cache)".to_string(),
            ));
        }

        // 2. Try to acquire distributed lock
        let _guard: Option<Box<crate::services::lock::LockGuard>> =
            match crate::services::lock::LockGuard::acquire_with_client(
                self.client.as_ref(),
                &format!("cache:lock:{}", key),
                lock_expiry_secs,
                max_retries,
                retry_delay,
            )
            .await
            {
                Ok(Some(AcquireResult::Acquired(guard))) => {
                    // Lock acquired — only this request will recompute.

                    // 3a. Double-check: the winning request might have populated the cache
                    //     between our step 1 and acquiring the lock.
                    if self.is_available()
                        && let Some(val) = self.get::<T>(key).await?
                    {
                        // guard dropped here via `return`, releasing lock
                        return Ok(val);
                    }
                    // Keep guard alive: the lock stays held through recompute+populate
                    // (steps 4-5), preventing other contending requests from also
                    // recomputing while the cache is still stale.
                    Some(guard)
                }
                Ok(Some(AcquireResult::Contended)) | Ok(None) => {
                    // 3b. Lock not acquired — poll cache with short delays before giving
                    //     up.  The winning request should populate the cache soon, so
                    //     waiting here is better than immediately recomputing.
                    for _ in 0..max_retries {
                        tokio::time::sleep(retry_delay).await;
                        if self.is_available()
                            && let Some(val) = self.get::<T>(key).await?
                        {
                            return Ok(val);
                        }
                    }
                    None
                }
                Err(e) => {
                    tracing::warn!(
                        "get_or_insert_with_lock: lock acquire error for key '{}': {}",
                        key,
                        e
                    );
                    None
                }
            };

        // 4. Recompute (lock held if _guard is Some)
        let val = f
            .await
            .map_err(|e| CacheError::FallbackFailed(format!("Fallback failed: {}", e)))?;

        // 5. Populate cache (best-effort — failures are logged, not propagated)
        if let Err(e) = self.set(key, &val, ttl).await {
            tracing::warn!("Cache set failed for key '{}': {:?}", key, e);
        }
        Ok(val)
        // Lock released on _guard Drop (when Some)
    }

    // ── set / set_null / invalidate ──────────────────────────────────────

    /// Store a value with TTL.
    pub async fn set<T: Serialize>(&self, key: &str, val: &T, ttl: Duration) -> CacheResult<()> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(()),
        };

        let json =
            serde_json::to_string(val).map_err(|e| CacheError::Serialization(e.to_string()))?;
        bb8_redis::redis::cmd("SETEX")
            .arg(key)
            .arg(ttl.as_secs())
            .arg(json)
            .query_async::<()>(&mut *conn)
            .await?;
        Ok(())
    }

    /// Store a short-lived negative-cache marker.
    ///
    /// Used when a database query confirms an entity does not exist. Prevents
    /// repeated cache-penetration queries for non-existent keys (e.g., deleted
    /// users).
    pub async fn set_null(&self, key: &str, ttl: Duration) -> CacheResult<()> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(()),
        };
        bb8_redis::redis::cmd("SETEX")
            .arg(format!("{}:null", key))
            .arg(ttl.as_secs())
            .arg("1")
            .query_async::<()>(&mut *conn)
            .await?;
        Ok(())
    }

    /// Delete a key and its negative-cache marker.
    pub async fn invalidate(&self, key: &str) -> CacheResult<()> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(()),
        };

        bb8_redis::redis::cmd("DEL")
            .arg(key)
            .arg(format!("{}:null", key))
            .query_async::<()>(&mut *conn)
            .await?;
        Ok(())
    }

    /// Ping Redis to verify the pool is healthy.
    pub async fn ping(&self) -> CacheResult<()> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Err(CacheError::NotAvailable),
        };
        bb8_redis::redis::cmd("PING")
            .query_async::<String>(&mut *conn)
            .await?;
        Ok(())
    }

    // ── Internals ────────────────────────────────────────────────────────

    /// Get a connection from the pool, or `None` if unavailable.
    async fn conn(&self) -> Option<bb8::PooledConnection<'_, RedisConnectionManager>> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return None,
        };

        match pool.get().await {
            Ok(conn) => Some(conn),
            Err(e) => {
                match &e {
                    bb8::RunError::TimedOut => {
                        tracing::warn!(
                            "CacheService: connection pool exhausted (all {} connections busy)",
                            self.max_size,
                        );
                    }
                    bb8::RunError::User(redis_err) => {
                        tracing::warn!(
                            "CacheService: Redis connection error: {:?}. \
                             Check Redis server availability and network connectivity.",
                            redis_err
                        );
                    }
                }
                None
            }
        }
    }

    /// Check if a key exists in the cache.
    pub async fn exists(&self, key: &str) -> CacheResult<bool> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(false),
        };
        let count: u64 = bb8_redis::redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut *conn)
            .await?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_mode_when_no_redis() {
        let cache = CacheService::new("", 10).await;
        assert!(!cache.is_available());
        assert!(cache.get::<String>("any").await.unwrap().is_none());
        assert!(cache.set("k", &"v", Duration::from_secs(10)).await.is_ok());
        assert!(cache.invalidate("k").await.is_ok());
        assert!(cache.ping().await.is_err());
    }

    // ── Tests below require a running Redis instance ──────────────
    // Run with: cargo test --package webshelf-server -- --ignored
    //
    // CI has Redis on 127.0.0.1:6379 (no password), matching this URL.
    // Local setup: docker run -d -p 6379:6379 redis:7-alpine

    fn redis_url() -> &'static str {
        "redis://127.0.0.1:6379"
    }

    #[tokio::test]
    #[ignore]
    async fn test_set_and_get() {
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:set_and_get";

        // Ensure clean state
        cache.invalidate(key).await.unwrap();
        assert!(cache.get::<String>(key).await.unwrap().is_none());

        // Set and verify
        cache
            .set(key, &"hello cache", Duration::from_secs(60))
            .await
            .unwrap();
        let val: String = cache.get(key).await.unwrap().expect("should have value");
        assert_eq!(val, "hello cache");

        // Clean up
        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_populates_cache() {
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:get_or_insert_populates";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // First call: miss, fallback runs
        let compute = async { Ok::<_, String>("computed value".to_string()) };
        let val = cache
            .get_or_insert::<String, _, _>(key, ttl, compute)
            .await
            .expect("get_or_insert should succeed");
        assert_eq!(val, "computed value");

        // Second call: should read from cache (no fallback)
        let compute2 = async { Ok::<_, String>("should not be called".to_string()) };
        let val2 = cache
            .get_or_insert::<String, _, _>(key, ttl, compute2)
            .await
            .expect("second call should succeed");
        assert_eq!(val2, "computed value");

        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_fallback_error() {
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:get_or_insert_err";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // First call: fallback fails → error propagated (no null marker stored)
        let compute_fail = async { Err::<String, _>("entity not found".to_string()) };
        let result = cache
            .get_or_insert::<String, _, _>(key, ttl, compute_fail)
            .await;
        assert!(
            result.unwrap_err().to_string().contains("Fallback failed"),
            "should propagate fallback error"
        );

        // Second call: no null marker was stored, so fallback still runs
        let call_count = std::sync::atomic::AtomicU32::new(0);
        let compute = async {
            call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, String>("retry result".to_string())
        };
        let val = cache
            .get_or_insert::<String, _, _>(key, ttl, compute)
            .await
            .expect("second call should succeed");
        assert_eq!(val, "retry result");

        cache.invalidate(key).await.unwrap();
    }

    /// Negative caching via explicit [`CacheService::set_null`].
    /// This is the pattern used by `get_user` in `user.rs`.
    #[tokio::test]
    #[ignore]
    async fn test_explicit_negative_cache() {
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:explicit_negative";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // Store a null marker (simulating "entity does not exist")
        cache.set_null(key, ttl).await.unwrap();

        // get_or_insert should hit the null marker without running fallback
        let compute_should_not_run = async {
            panic!("fallback should not be called when null marker exists");
        };
        let result = cache
            .get_or_insert::<String, _, String>(key, ttl, compute_should_not_run)
            .await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("negative cache"),
            "should indicate negative cache hit"
        );

        // After invalidation, fallback runs again
        cache.invalidate(key).await.unwrap();
        let compute = async { Ok::<_, String>("fresh value".to_string()) };
        let val = cache
            .get_or_insert::<String, _, _>(key, ttl, compute)
            .await
            .expect("after invalidation, should recompute");
        assert_eq!(val, "fresh value");

        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_invalidate_clears_key_and_null_marker() {
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:invalidate_both";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // Set a value
        cache.set(key, &"data", ttl).await.unwrap();
        let val: String = cache.get(key).await.unwrap().expect("should exist");
        assert_eq!(val, "data");

        // Set a null marker
        cache.set_null(key, ttl).await.unwrap();

        // Invalidate should clear both key and key:null
        cache.invalidate(key).await.unwrap();
        assert!(
            cache.get::<String>(key).await.unwrap().is_none(),
            "key should be deleted"
        );
        assert!(
            !cache.exists(&format!("{}:null", key)).await.unwrap(),
            "null marker should also be deleted"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_with_lock_populates_cache() {
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:get_or_insert_with_lock_populates";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // First call: miss, lock acquired, fallback runs
        let compute = async { Ok::<_, String>("computed via lock".to_string()) };
        let val = cache
            .get_or_insert_with_lock::<String, _, _>(
                key,
                ttl,
                5,                         // lock expiry
                Duration::from_millis(50), // retry delay
                3,                         // max retries
                compute,
            )
            .await
            .expect("get_or_insert_with_lock should succeed");
        assert_eq!(val, "computed via lock");

        // Second call: should read from cache (fallback should NOT run)
        let compute_should_not_run = async {
            panic!("fallback should not be called on cache hit");
            #[allow(unreachable_code)]
            Ok::<_, String>("unreachable".to_string())
        };
        let val2 = cache
            .get_or_insert_with_lock::<String, _, _>(
                key,
                ttl,
                5,
                Duration::from_millis(50),
                3,
                compute_should_not_run,
            )
            .await
            .expect("second call should read from cache");
        assert_eq!(val2, "computed via lock");

        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_with_lock_fallback_error() {
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:get_or_insert_with_lock_err";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // Fallback fails → error propagated
        let compute_fail = async { Err::<String, _>("entity not found".to_string()) };
        let result = cache
            .get_or_insert_with_lock::<String, _, _>(
                key,
                ttl,
                5,
                Duration::from_millis(50),
                3,
                compute_fail,
            )
            .await;
        assert!(
            result.unwrap_err().to_string().contains("Fallback failed"),
            "should propagate fallback error"
        );

        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_with_lock_noop_when_redis_unavailable() {
        // When CacheService is in no-op mode, get_or_insert_with_lock should
        // fall back to lock-free computation (no panic, no crash).
        let cache = CacheService::new("", 10).await;
        assert!(!cache.is_available());

        let key = "test:noop_lock";
        let ttl = Duration::from_secs(60);
        let compute = async { Ok::<_, String>("noop result".to_string()) };
        let val = cache
            .get_or_insert_with_lock::<String, _, _>(
                key,
                ttl,
                5,
                Duration::from_millis(50),
                3,
                compute,
            )
            .await
            .expect("should succeed in no-op mode");
        assert_eq!(val, "noop result");
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_with_lock_concurrent_stampede_protection() {
        // Verifies that when N tasks concurrently access the same key,
        // the fallback function executes exactly once (stampede protection).
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:stampede";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        let fallback_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let num_tasks: u32 = 8;

        let mut handles = Vec::with_capacity(num_tasks as usize);
        for _ in 0..num_tasks {
            let cache = cache.clone();
            let key = key.to_string();
            let fallback_count = fallback_count.clone();
            handles.push(tokio::spawn(async move {
                // Each task independently calls get_or_insert_with_lock.
                // Only one should acquire the lock and run the fallback;
                // others should wait and read from cache.
                let compute = async {
                    fallback_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    Ok::<_, String>("stampede protected".to_string())
                };
                cache
                    .get_or_insert_with_lock::<String, _, String>(
                        &key,
                        ttl,
                        5,                         // lock expiry
                        Duration::from_millis(50), // retry delay
                        10,                        // max retries
                        compute,
                    )
                    .await
                    .expect("get_or_insert_with_lock should succeed")
            }));
        }

        // Wait for all tasks and collect results
        use futures::future::join_all;
        let results = join_all(handles).await;
        for (i, result) in results.iter().enumerate() {
            let val = result.as_ref().expect("task should not panic");
            assert_eq!(
                val.as_str(),
                "stampede protected",
                "task {} got wrong value",
                i
            );
        }

        // Verify fallback ran exactly once
        assert_eq!(
            fallback_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "fallback should execute exactly once under concurrent access"
        );

        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_with_lock_negative_cache() {
        // 验证当 :null 标记存在时，get_or_insert_with_lock
        // 返回 CacheError::FallbackFailed 且不回源执行 fallback。
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:get_or_insert_with_lock_neg";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // Store a null marker (simulating "entity does not exist")
        cache.set_null(key, ttl).await.unwrap();

        // get_or_insert_with_lock should hit the null marker without running fallback
        let compute_should_not_run = async {
            panic!("fallback should not be called when null marker exists");
            #[allow(unreachable_code)]
            Ok::<_, String>("unreachable".to_string())
        };
        let result = cache
            .get_or_insert_with_lock::<String, _, String>(
                key,
                ttl,
                5,
                Duration::from_millis(50),
                3,
                compute_should_not_run,
            )
            .await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("negative cache"),
            "should indicate negative cache hit"
        );

        // After invalidation, fallback runs again
        cache.invalidate(key).await.unwrap();
        let compute = async { Ok::<_, String>("fresh value".to_string()) };
        let val = cache
            .get_or_insert_with_lock::<String, _, _>(
                key,
                ttl,
                5,
                Duration::from_millis(50),
                3,
                compute,
            )
            .await
            .expect("after invalidation, should recompute");
        assert_eq!(val, "fresh value");

        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_with_lock_prefix_no_conflict() {
        // 验证 key 中包含 "cache:lock:" 前缀不会与内部锁 key 冲突。
        // 内部锁 key 为 "cache:lock:{用户key}"，因此当应用层 key 本身
        // 就是 "cache:lock:something" 时，锁 key 为
        // "cache:lock:cache:lock:something"，两者截然不同。
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "cache:lock:my_resource";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        // First call: miss, should recompute and store
        let compute = async { Ok::<_, String>("prefix safe".to_string()) };
        let val = cache
            .get_or_insert_with_lock::<String, _, _>(
                key,
                ttl,
                5,
                Duration::from_millis(50),
                3,
                compute,
            )
            .await
            .expect("should succeed even with cache:lock: prefix in key");
        assert_eq!(val, "prefix safe");

        // Second call: should read from cache
        let compute_should_not_run = async {
            panic!("fallback should not be called on cache hit");
            #[allow(unreachable_code)]
            Ok::<_, String>("unreachable".to_string())
        };
        let val2 = cache
            .get_or_insert_with_lock::<String, _, _>(
                key,
                ttl,
                5,
                Duration::from_millis(50),
                3,
                compute_should_not_run,
            )
            .await
            .expect("second call should read from cache");
        assert_eq!(val2, "prefix safe");

        cache.invalidate(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_or_insert_with_lock_high_contention() {
        // 在高锁竞争条件下验证所有并发任务最终获得相同结果且不 panic。
        // 使用极短的 lock_expiry_secs（1s）和较长的回源计算（200ms），
        // 锁可能在 winner 完成前就过期，导致多个任务回源计算。
        // 与 stampede 测试不同，这里不强求 fallback 只执行 1 次。
        let cache = CacheService::new(redis_url(), 10).await;
        let key = "test:high_contention";
        let ttl = Duration::from_secs(60);

        // Ensure clean state
        cache.invalidate(key).await.unwrap();

        let num_tasks: u32 = 8;
        let mut handles = Vec::with_capacity(num_tasks as usize);
        for _ in 0..num_tasks {
            let cache = cache.clone();
            let key = key.to_string();
            handles.push(tokio::spawn(async move {
                let compute = async {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    Ok::<_, String>("high contention result".to_string())
                };
                cache
                    .get_or_insert_with_lock::<String, _, String>(
                        &key,
                        ttl,
                        1,                         // lock expiry: 1 second
                        Duration::from_millis(10), // retry delay: 10ms
                        50,                        // max retries: 50
                        compute,
                    )
                    .await
                    .expect("get_or_insert_with_lock should succeed")
            }));
        }

        // Wait for all tasks and collect results
        use futures::future::join_all;
        let results = join_all(handles).await;
        for (i, result) in results.iter().enumerate() {
            let val = result.as_ref().expect("task should not panic");
            assert_eq!(
                val.as_str(),
                "high contention result",
                "task {} got wrong value",
                i
            );
        }

        cache.invalidate(key).await.unwrap();
    }
}
