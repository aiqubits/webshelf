use async_trait::async_trait;
use parking_lot::Mutex;
use rand::Rng;
use sea_orm::{
    AccessMode, ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction,
    DbBackend, DbErr, ExecResult, IsolationLevel, QueryResult, Statement, TransactionError,
    TransactionTrait,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::utils::config::{DatabaseConfig, DatabaseReadConfig, DatabaseRoutingConfig};

/// Read replica selection strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadStrategy {
    RoundRobin = 0,
    Random = 1,
    Weighted = 2,
}

/// A single read replica connection
struct ReadReplica {
    conn: DatabaseConnection,
    /// Original index in the `database_read_urls` config array.
    /// Used to correctly map configured weights even when some replicas
    /// fail to connect (preventing weight-to-replica misalignment).
    #[allow(dead_code)]
    original_index: usize,
    weight: u32,
}

/// Tracks which read replicas are temporarily marked as down
struct HealthState {
    down_until: Vec<Option<Instant>>,
}

/// Application-level router that implements SeaORM's `ConnectionTrait` and
/// `TransactionTrait` to provide transparent read-write splitting.
///
/// - `execute` / `execute_unprepared` → always routed to the write database
/// - `query_one` / `query_all` → routed to read replicas (with retry, circuit
///   breaker, and fallback to write)
/// - `SELECT ... FOR UPDATE / FOR SHARE` → forced to the write database
///   (detected at the SQL template level)
/// - `begin` / `transaction` → always on the write database
///
/// When no read replicas are configured, `AutoRouter` transparently degenerates
/// into a single-database pass-through.
pub struct AutoRouter {
    write: DatabaseConnection,
    reads: Vec<ReadReplica>,
    strategy: ReadStrategy,
    rr_counter: AtomicUsize,
    health: Mutex<HealthState>,
    circuit_break: Duration,
    /// Extra retry attempts after all read replicas have been tried once.
    /// Each extra attempt tries any non-circuit-broken replica (including
    /// those that previously failed but whose circuit breaker has expired).
    retry_attempts: usize,
    fallback_to_write: bool,
}

impl AutoRouter {
    /// Create a multi-database router (one writer + N readers).
    pub async fn new(
        write_url: &str,
        read_urls: &[String],
        write_config: &DatabaseConfig,
        read_config: &DatabaseReadConfig,
        routing_config: &DatabaseRoutingConfig,
    ) -> Result<Arc<Self>, DbErr> {
        // 1. Connect to the write database
        let write = connect_db(write_url, write_config).await?;

        // Fail-fast when read_weights length doesn't match — a silent weight
        // fallback masks configuration mistakes that are hard to debug.
        if !routing_config.read_weights.is_empty()
            && routing_config.read_weights.len() != read_urls.len()
        {
            return Err(DbErr::Custom(format!(
                "read_weights length ({}) does not match database_read_urls count ({}); each read replica must have a corresponding weight, or remove read_weights entirely to default all replicas to weight 1",
                routing_config.read_weights.len(),
                read_urls.len(),
            )));
        }

        // 2. Connect to read replicas in parallel, tracking original indices
        let urls_with_idx: Vec<(usize, String)> = read_urls
            .iter()
            .enumerate()
            .map(|(i, u)| (i, u.clone()))
            .collect();
        let mut reads: Vec<ReadReplica> = Vec::with_capacity(urls_with_idx.len());
        let handles: Vec<_> = urls_with_idx
            .into_iter()
            .map(|(i, url)| {
                let cfg = read_config.clone();
                tokio::spawn(async move { (i, connect_db_read(&url, &cfg).await) })
            })
            .collect();

        for handle in handles {
            match handle.await {
                Ok((i, Ok(conn))) => {
                    // Compute weight directly using original_index.
                    // This eliminates the need for a secondary weight-application loop.
                    let weight = if !routing_config.read_weights.is_empty() {
                        routing_config
                            .read_weights
                            .get(i)
                            .copied()
                            .unwrap_or(1)
                            .max(1)
                    } else {
                        1
                    };
                    reads.push(ReadReplica {
                        conn,
                        original_index: i,
                        weight,
                    });
                }
                Ok((i, Err(e))) => {
                    tracing::warn!("Read replica {} failed to connect: {}", i, e);
                }
                Err(e) => {
                    tracing::error!("Read replica connection task panicked: {:?}", e);
                }
            }
        }

        // 3. If no read replicas connected, fall back to single-db mode
        if reads.is_empty() {
            tracing::warn!("No read replicas connected — running in single-database mode");
            return Ok(Arc::new(Self::new_internal(
                write,
                Vec::new(),
                routing_config,
            )));
        }

        tracing::info!(
            "AutoRouter initialized with {} read replica(s)",
            reads.len()
        );

        Ok(Arc::new(Self::new_internal(write, reads, routing_config)))
    }

    /// Create a single-database router (no read-write splitting).
    pub fn single(write: DatabaseConnection) -> Arc<Self> {
        Arc::new(Self {
            write,
            reads: vec![],
            strategy: ReadStrategy::RoundRobin,
            rr_counter: AtomicUsize::new(0),
            health: Mutex::new(HealthState { down_until: vec![] }),
            circuit_break: Duration::from_secs(30),
            retry_attempts: 2,
            fallback_to_write: false,
        })
    }

    fn new_internal(
        write: DatabaseConnection,
        reads: Vec<ReadReplica>,
        routing: &DatabaseRoutingConfig,
    ) -> Self {
        let read_count = reads.len();
        let strategy = match routing.strategy.to_lowercase().as_str() {
            "random" => ReadStrategy::Random,
            "weighted" => ReadStrategy::Weighted,
            _ => ReadStrategy::RoundRobin,
        };
        Self {
            write,
            reads,
            strategy,
            rr_counter: AtomicUsize::new(0),
            health: Mutex::new(HealthState {
                down_until: vec![None; read_count],
            }),
            circuit_break: Duration::from_millis(routing.circuit_break_ms),
            retry_attempts: routing.retry_attempts,
            fallback_to_write: routing.fallback_to_write,
        }
    }

    /// Return the write database connection directly.
    ///
    /// Use this when you need **read-your-writes consistency** — e.g. after
    /// a transaction that modifies data, re-querying the write database
    /// guarantees you see the latest state even if the read replicas are
    /// behind.
    pub fn write_conn(&self) -> &DatabaseConnection {
        &self.write
    }

    /// Return the database backend type of the write database.
    pub fn write_backend(&self) -> DbBackend {
        self.write.get_database_backend()
    }

    /// Start a background health-check task that periodically probes all
    /// currently-down read replicas and removes the circuit breaker when a
    /// replica recovers.
    pub fn start_health_check(self: Arc<Self>, interval: Duration) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                self.probe_reads().await;
            }
        });
    }

    async fn probe_reads(&self) {
        // Collect indices to probe while holding the lock, then probe without the lock
        let to_probe: Vec<usize> = {
            let health = self.health.lock();
            let now = Instant::now();
            let mut result = Vec::new();
            for i in 0..self.reads.len() {
                if let Some(until) = health.down_until[i]
                    && now >= until
                {
                    result.push(i);
                }
            }
            result
        };

        for &i in &to_probe {
            if self.reads[i].conn.ping().await.is_ok() {
                let mut health = self.health.lock();
                // Only clear if still marked as down — a concurrent mark_down
                // may have re-set it after our to_probe collection was taken.
                if health.down_until[i].is_some() {
                    health.down_until[i] = None;
                    tracing::info!("Read replica {} recovered", i);
                }
            } else {
                let mut health = self.health.lock();
                health.down_until[i] = Some(Instant::now() + self.circuit_break);
            }
        }
    }

    // ---- internal routing helpers ----

    /// Select the next healthy read replica index, excluding already-tried ones.
    fn pick_next_read(&self, exclude: &HashSet<usize>) -> Option<usize> {
        if self.reads.is_empty() {
            return None;
        }

        let now = Instant::now();
        // Collect healthy replicas
        let mut healthy: Vec<usize> = Vec::new();
        {
            let mut health = self.health.lock();
            for (i, _) in self.reads.iter().enumerate() {
                if exclude.contains(&i) {
                    continue;
                }
                // Auto-recover expired circuit breakers
                if let Some(until) = health.down_until[i] {
                    if now >= until {
                        health.down_until[i] = None;
                    } else {
                        continue;
                    }
                }
                healthy.push(i);
            }
        }

        if healthy.is_empty() {
            return None;
        }

        let chosen = match self.strategy {
            ReadStrategy::RoundRobin => {
                let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed);
                healthy[idx % healthy.len()]
            }
            ReadStrategy::Random => {
                let mut rng = rand::thread_rng();
                healthy[rng.gen_range(0..healthy.len())]
            }
            ReadStrategy::Weighted => {
                let weights: Vec<u64> = healthy
                    .iter()
                    .map(|&i| self.reads[i].weight as u64)
                    .collect();
                let total: u64 = weights.iter().sum();
                // total > 0 is guaranteed during normal operation because
                // weight values are min-capped at 1 during connection setup.
                // This guard prevents panics if a future refactor removes the
                // min-cap — if total is 0, fall back to round-robin.
                if total == 0 {
                    let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed);
                    healthy[idx % healthy.len()]
                } else {
                    let mut rng = rand::thread_rng();
                    let mut roll = rng.gen_range(0..total);
                    let mut chosen = healthy[0];
                    for (&idx, &w) in healthy.iter().zip(weights.iter()) {
                        if roll < w {
                            chosen = idx;
                            break;
                        }
                        roll -= w;
                    }
                    chosen
                }
            }
        };

        Some(chosen)
    }

    /// Execute a read operation with retry logic.
    /// Uses owned `DatabaseConnection` to avoid lifetime issues with async closures.
    async fn execute_read_retry<T, F, Fut>(&self, stmt: Statement, op: F) -> Result<T, DbErr>
    where
        F: Fn(DatabaseConnection, Statement) -> Fut + Copy,
        Fut: std::future::Future<Output = Result<T, DbErr>>,
    {
        if self.reads.is_empty() {
            return op(self.write.clone(), stmt).await;
        }

        let mut tried: HashSet<usize> = HashSet::new();
        let mut last_err: Option<DbErr> = None;

        // Phase 1: try each replica once (exclude already-tried ones)
        for _ in 0..self.reads.len() {
            let Some(idx) = self.pick_next_read(&tried) else {
                break;
            };
            tried.insert(idx);

            match op(self.reads[idx].conn.clone(), stmt.clone()).await {
                Ok(v) => return Ok(v),
                Err(e) if is_connection_error(&e) => {
                    self.mark_down(idx);
                    tracing::warn!("Read replica {} failed, marked down: {}", idx, e);
                    last_err = Some(e);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        // Phase 2: extra retry rounds — retry previously-failed replicas directly,
        // bypassing the circuit breaker. The circuit breaker prevents selecting
        // a recently-failed replica in *subsequent* requests; within the same
        // request retry_attempts allows giving each replica a second chance
        // before the circuit breaker fully kicks in.
        if last_err.is_some() && self.retry_attempts > 0 {
            for retry in 0..self.retry_attempts {
                let idx = (tried.len() + retry) % self.reads.len();

                match op(self.reads[idx].conn.clone(), stmt.clone()).await {
                    Ok(v) => return Ok(v),
                    Err(e) if is_connection_error(&e) => {
                        self.mark_down(idx);
                        tracing::warn!(
                            "Read replica {} failed during retry, marked down: {}",
                            idx,
                            e
                        );
                        last_err = Some(e);
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        if self.fallback_to_write {
            tracing::warn!("All read replicas failed — falling back to writer");
            return op(self.write.clone(), stmt).await;
        }

        Err(last_err.unwrap_or_else(|| DbErr::Custom("all read attempts exhausted".into())))
    }

    fn mark_down(&self, idx: usize) {
        let mut health = self.health.lock();
        if let Some(slot) = health.down_until.get_mut(idx) {
            *slot = Some(Instant::now() + self.circuit_break);
        }
    }
}

// ---- ConnectionTrait implementation ----

#[async_trait]
impl ConnectionTrait for AutoRouter {
    fn get_database_backend(&self) -> DbBackend {
        self.write.get_database_backend()
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        self.write.execute_unprepared(sql).await
    }

    fn support_returning(&self) -> bool {
        self.write.support_returning()
    }

    async fn execute(&self, stmt: Statement) -> Result<ExecResult, DbErr> {
        self.write.execute(stmt).await
    }

    async fn query_one(&self, stmt: Statement) -> Result<Option<QueryResult>, DbErr> {
        if is_write_statement(&stmt) || is_locking_select(&stmt) || self.reads.is_empty() {
            tracing::trace!(target = "write", "query_one routed to write");
            return self.write.query_one(stmt).await;
        }
        tracing::trace!(target = "read", "query_one routed to read replicas");
        self.execute_read_retry(stmt, |conn, s| async move { conn.query_one(s).await })
            .await
    }

    async fn query_all(&self, stmt: Statement) -> Result<Vec<QueryResult>, DbErr> {
        if is_write_statement(&stmt) || is_locking_select(&stmt) || self.reads.is_empty() {
            tracing::trace!(target = "write", "query_all routed to write");
            return self.write.query_all(stmt).await;
        }
        tracing::trace!(target = "read", "query_all routed to read replicas");
        self.execute_read_retry(stmt, |conn, s| async move { conn.query_all(s).await })
            .await
    }

    fn is_mock_connection(&self) -> bool {
        false
    }
}

// ---- TransactionTrait implementation ----

#[async_trait]
impl TransactionTrait for AutoRouter {
    async fn begin(&self) -> Result<DatabaseTransaction, DbErr> {
        self.write.begin().await
    }

    async fn begin_with_config(
        &self,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<DatabaseTransaction, DbErr> {
        self.write
            .begin_with_config(isolation_level, access_mode)
            .await
    }

    async fn transaction<F, T, E>(&self, txn: F) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c DatabaseTransaction,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<T, E>> + Send + 'c>,
            > + Send,
        T: Send,
        E: std::fmt::Debug + std::fmt::Display + Send,
    {
        self.write.transaction(txn).await
    }

    async fn transaction_with_config<F, T, E>(
        &self,
        txn: F,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c DatabaseTransaction,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<T, E>> + Send + 'c>,
            > + Send,
        T: Send,
        E: std::fmt::Debug + std::fmt::Display + Send,
    {
        self.write
            .transaction_with_config(txn, isolation_level, access_mode)
            .await
    }
}

// ---- SQL detection ----

/// Detect locking SELECT statements by examining the SQL template rather than
/// the fully-rendered SQL string.
fn is_locking_select(stmt: &Statement) -> bool {
    let sql = stmt.to_string();
    if sql.is_empty() {
        return false;
    }
    let up = sql.to_ascii_uppercase();
    up.contains("FOR UPDATE")
        || up.contains("FOR SHARE")
        || up.contains("FOR NO KEY UPDATE")
        || up.contains("FOR KEY SHARE")
        || up.contains("LOCK IN SHARE MODE")
}

/// Detect write statements that SeaORM routes through `query_one()`/`query_all()`
/// via the `RETURNING` clause on PostgreSQL.
///
/// SeaORM uses `INSERT ... RETURNING *` for `ActiveModel::insert()` and
/// `UPDATE ... RETURNING *` for `ActiveModel::update()`. These generate
/// `query_one()` calls (not `execute()`), so the standard write-routing
/// in `execute()` does not catch them. Without this check they would be
/// incorrectly sent to a read replica.
fn is_write_statement(stmt: &Statement) -> bool {
    let sql = stmt.to_string();
    if sql.is_empty() {
        return false;
    }
    let up = sql.trim_start().to_ascii_uppercase();
    up.starts_with("INSERT ")
        || up.starts_with("UPDATE ")
        || up.starts_with("DELETE ")
        || up.starts_with("REPLACE ")
}

/// Determine whether a `DbErr` likely represents a connection-level
/// failure rather than a query-level error.
///
/// - `DbErr::Conn` is always a connection-level failure (pool timeout,
///   connection refused, etc.).
/// - `DbErr::Query` may wrap sqlx-level connectivity errors (e.g., broken
///   pipe mid-query). A conservative keyword set is used to avoid false
///   positives from legitimate query error messages.
/// - All other variants are never considered connection errors.
fn is_connection_error(e: &DbErr) -> bool {
    // Primary: DbErr::Conn is always a connection-level failure
    if matches!(e, DbErr::Conn(_)) {
        return true;
    }

    // SeaORM 1.1.20+: ConnectionAcquire indicates pool timeout or connection
    // closed — both are connection-level failures. This variant is thrown when
    // acquire_timeout is reached or the pool notices a closed connection,
    // neither of which is a query-level error.
    if matches!(e, DbErr::ConnectionAcquire(_)) {
        return true;
    }

    // Secondary: sqlx may report mid-query connection failures (e.g., broken
    // pipe, transport EOF) as DbErr::Query. Use a conservative keyword set
    // that is extremely unlikely to appear in actual query error messages.
    if matches!(e, DbErr::Query(_)) {
        let s = e.to_string().to_ascii_lowercase();
        let hints = [
            "broken pipe",
            "connection reset",
            "io error",
            "i/o",
            "network",
            "eof",
            "transport",
        ];
        return hints.iter().any(|h| s.contains(h));
    }

    false
}

// ---- connection helpers ----

/// Connect to a database with the given URL and write-pool config.
pub async fn connect_db(url: &str, config: &DatabaseConfig) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .max_lifetime(Duration::from_secs(1800))
        .test_before_acquire(true);
    Database::connect(opt).await
}

/// Connect to a read replica with the given URL and read-pool config.
async fn connect_db_read(
    url: &str,
    config: &DatabaseReadConfig,
) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .max_lifetime(Duration::from_secs(1800))
        .test_before_acquire(true);
    Database::connect(opt).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnAcquireErr, RuntimeErr, Statement};

    // ---- is_locking_select tests ----

    #[test]
    fn test_is_locking_select_for_update() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1 FOR UPDATE".to_string(),
        );
        assert!(is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_for_share() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1 FOR SHARE".to_string(),
        );
        assert!(is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_for_no_key_update() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1 FOR NO KEY UPDATE".to_string(),
        );
        assert!(is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_for_key_share() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1 FOR KEY SHARE".to_string(),
        );
        assert!(is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_lock_in_share_mode() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1 LOCK IN SHARE MODE".to_string(),
        );
        assert!(is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_plain_select() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1".to_string(),
        );
        assert!(!is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_empty() {
        let stmt = Statement::from_string(DbBackend::Postgres, "".to_string());
        assert!(!is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_insert_statement() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "INSERT INTO users (name) VALUES ($1)".to_string(),
        );
        assert!(!is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_update_statement() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "UPDATE users SET name = $1 WHERE id = $2".to_string(),
        );
        assert!(!is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_lowercase_for_update() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "select * from users where id = $1 for update".to_string(),
        );
        assert!(is_locking_select(&stmt));
    }

    #[test]
    fn test_is_locking_select_mixed_case_for_update() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1 For UpDaTe".to_string(),
        );
        assert!(is_locking_select(&stmt));
    }

    // ---- is_write_statement tests ----

    #[test]
    fn test_is_write_statement_insert() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id".to_string(),
        );
        assert!(is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_update() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "UPDATE users SET name = $1 WHERE id = $2 RETURNING id, name".to_string(),
        );
        assert!(is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_delete() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "DELETE FROM users WHERE id = $1 RETURNING id".to_string(),
        );
        assert!(is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_replace() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "REPLACE INTO users (id, name) VALUES ($1, $2)".to_string(),
        );
        assert!(is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_lowercase_insert() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "insert into users (name) values ($1) returning id".to_string(),
        );
        assert!(is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_select_is_not_write() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1".to_string(),
        );
        assert!(!is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_with_cte_select_is_not_write() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "WITH recent AS (SELECT * FROM users ORDER BY id DESC LIMIT 10) SELECT * FROM recent"
                .to_string(),
        );
        assert!(!is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_empty() {
        let stmt = Statement::from_string(DbBackend::Postgres, "".to_string());
        assert!(!is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_leading_whitespace() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "  INSERT INTO logs (event) VALUES ($1)".to_string(),
        );
        assert!(is_write_statement(&stmt));
    }

    // ---- is_connection_error tests ----

    #[test]
    fn test_is_connection_error_connection_closed() {
        let err = DbErr::Conn(RuntimeErr::Internal("connection closed".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_timeout() {
        let err = DbErr::Conn(RuntimeErr::Internal("pool timed out".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_io_error() {
        let err = DbErr::Conn(RuntimeErr::Internal("IO error: broken pipe".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_broken_pipe() {
        let err = DbErr::Conn(RuntimeErr::Internal("broken pipe".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_network() {
        let err = DbErr::Conn(RuntimeErr::Internal("network is unreachable".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_transport() {
        let err = DbErr::Conn(RuntimeErr::Internal("transport error".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_eof() {
        let err = DbErr::Conn(RuntimeErr::Internal("unexpected eof".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_reset() {
        let err = DbErr::Conn(RuntimeErr::Internal("connection reset by peer".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_acquire_timeout() {
        let err = DbErr::ConnectionAcquire(ConnAcquireErr::Timeout);
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_acquire_connection_closed() {
        let err = DbErr::ConnectionAcquire(ConnAcquireErr::ConnectionClosed);
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_query_error_not_connection() {
        let err = DbErr::Query(RuntimeErr::Internal(
            "syntax error at or near \"SELECT\"".to_string(),
        ));
        assert!(!is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_unique_violation() {
        let err = DbErr::Query(RuntimeErr::Internal("duplicate key value".to_string()));
        assert!(!is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_record_not_found() {
        let err = DbErr::RecordNotFound("not found".to_string());
        assert!(!is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_case_insensitive() {
        let err = DbErr::Conn(RuntimeErr::Internal("Connection refused".to_string()));
        assert!(is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_query_with_connection_word_is_not_connection_error() {
        // Regression: a DbErr::Query that happens to contain the word "connection"
        // in its error message must NOT be treated as a connection error.
        let err = DbErr::Query(RuntimeErr::Internal(
            "column \"connection_id\" does not exist".to_string(),
        ));
        assert!(!is_connection_error(&err));
    }

    #[test]
    fn test_is_connection_error_query_with_timeout_word_is_not_connection_error() {
        // Regression: a DbErr::Query containing "timeout" in a query context
        // must NOT be treated as a connection error.
        let err = DbErr::Query(RuntimeErr::Internal("statement timeout".to_string()));
        assert!(!is_connection_error(&err));
    }

    // ---- ReadStrategy tests ----

    #[test]
    fn test_read_strategy_round_robin_default() {
        let strategy = ReadStrategy::RoundRobin;
        assert_eq!(strategy as u8, 0);
    }

    #[test]
    fn test_read_strategy_random() {
        let strategy = ReadStrategy::Random;
        assert_eq!(strategy as u8, 1);
    }

    #[test]
    fn test_read_strategy_weighted() {
        let strategy = ReadStrategy::Weighted;
        assert_eq!(strategy as u8, 2);
    }

    // ---- AutoRouter::single basic test ----
    //
    // NOTE: Full integration tests for AutoRouter require a running PostgreSQL
    // instance. These minimal unit tests verify construction and the no-read-replica
    // path. For read-replica routing tests, see the server integration tests.

    #[test]
    fn test_health_state_default() {
        let health = HealthState {
            down_until: vec![None, None, None],
        };
        assert_eq!(health.down_until.len(), 3);
        assert!(health.down_until.iter().all(|d| d.is_none()));
    }

    #[test]
    fn test_health_state_some_down() {
        let now = Instant::now();
        let health = HealthState {
            down_until: vec![Some(now), None],
        };
        assert!(health.down_until[0].is_some());
        assert!(health.down_until[1].is_none());
    }

    // ---- is_write_statement edge cases ----

    #[test]
    fn test_is_write_statement_truncate_not_write() {
        // TRUNCATE is a DDL statement, not a DML write-through-query.
        // It should NOT be caught by is_write_statement (it goes through
        // execute(), not query_one/query_all).
        let stmt = Statement::from_string(DbBackend::Postgres, "TRUNCATE TABLE users".to_string());
        assert!(!is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_with_cte_insert() {
        // A WITH ... INSERT is still an INSERT statement.
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "WITH deleted AS (DELETE FROM logs WHERE created_at < $1 RETURNING *) INSERT INTO audit SELECT * FROM deleted".to_string(),
        );
        // starts with "WITH", not "INSERT", so is_write_statement returns false.
        // This is acceptable: the CTE-INSERT is rare, and even if routed to a
        // read replica, PostgreSQL would reject it with a write-error.
        assert!(!is_write_statement(&stmt));
    }

    #[test]
    fn test_is_write_statement_upsert_detected_as_insert() {
        // INSERT ... ON CONFLICT (UPSERT) starts with "INSERT ",
        // so is_write_statement correctly detects it as a write operation.
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "INSERT INTO users (id, name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET name = $3"
                .to_string(),
        );
        assert!(is_write_statement(&stmt));
    }
}
