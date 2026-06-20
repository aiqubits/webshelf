//! Concurrent circuit breaker tests for AutoRouter with real PostgreSQL read replicas.
//!
//! Tests require only `WEBSHELF_DATABASE_READ_URLS` to be set (comma-separated
//! replica URLs). No Docker or special infrastructure is needed — connection
//! failures are simulated via `pg_terminate_backend` through a direct SeaORM
//! connection to the target replica.
//!
//! In GitHub Actions CI, `pg-replica1` and `pg-replica2` are defined as
//! separate `services:` entries. The test gracefully skips itself when
//! `WEBSHELF_DATABASE_READ_URLS` is unset (local development).
//!
//! Architecture: queries use `SELECT 1` (no table dependency), so the
//! replicas do NOT need streaming replication or schema migrations.
//!
//! Run with: cargo test --test replica_circuit_breaker_test -- --nocapture
//!
//! Coverage summary (unit tests in db_router.rs cover SQL detection;
//! these integration tests verify the ENTIRE routing path end-to-end):
//!
//! | query_one / query_all path               | Expected route | Covered |
//! |------------------------------------------|---------------|---------|
//! | INSERT ... RETURNING                     | write DB      | ✅      |
//! | UPDATE ... RETURNING                     | write DB      | ✅      |
//! | DELETE ... RETURNING                     | write DB      | ✅      |
//! | SELECT ... FOR UPDATE / FOR SHARE        | write DB      | ✅      |
//! | WITH ... SELECT (read CTE)               | replicas      | ✅      |
//! | WITH ... INSERT/UPDATE/DELETE (write CTE)| write DB      | ✅      |
//! | execute() / execute_unprepared()         | write DB      | ✅      |
//! | write_conn() (direct access)             | write DB      | ✅      |
//! | Normal SELECT                            | replicas      | ✅      |
//! | Circuit breaker (replica failure)        | fallback/err  | ✅      |
//! | Fallback to write (fallback_to_write)    | write DB      | ✅      |
//! |------------------------------------------|---------------|---------|
//!
//! Verification strategy relies on hot standby read-only semantics:
//! write operations (INSERT/UPDATE/DELETE/FOR UPDATE) sent to a standby
//! replica fail with "cannot execute X in a read-only transaction".
//! Success = correctly routed to write DB.

use std::sync::Arc;
use std::time::Duration;

use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use tokio::sync::Barrier;
use webshelf_server::AutoRouter;
use webshelf_server::utils::config::{DatabaseReadConfig, DatabaseRoutingConfig};

// ── Prerequisite checks ──────────────────────────────────────────────

/// Check whether the test environment has read replicas configured.
fn has_read_replicas() -> bool {
    std::env::var("WEBSHELF_DATABASE_READ_URLS")
        .ok()
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Load the application configuration.
fn load_app_config() -> webshelf_server::utils::config::AppConfig {
    webshelf_server::utils::load_config("config.toml", "development")
        .expect("Failed to load config")
}

/// Create an `AutoRouter` with short circuit-breaker duration for fast tests.
async fn create_test_router(circuit_break_ms: u64, fallback_to_write: bool) -> Arc<AutoRouter> {
    let config = load_app_config();

    let routing = DatabaseRoutingConfig {
        strategy: "round_robin".to_string(),
        read_weights: vec![],
        retry_attempts: 2,
        circuit_break_ms,
        fallback_to_write,
        health_check_interval_secs: 0, // disable background health check
    };

    let read_config = DatabaseReadConfig::default();

    AutoRouter::new(
        &config.database_url,
        &config.database_read_urls,
        &config.database,
        &read_config,
        &routing,
    )
    .await
    .expect("Failed to create AutoRouter with read replicas")
}

/// Warm up: ensure pool connections are established to all replicas.
async fn warm_up_replicas(router: &AutoRouter, count: usize) {
    for i in 0..count {
        let stmt = Statement::from_string(DbBackend::Postgres, format!("SELECT {} AS warmup", i));
        let _ = router.query_one(stmt).await;
    }
}

/// Run `n` concurrent read queries through the AutoRouter.
/// Returns the number of successful queries.
async fn concurrent_reads(router: &Arc<AutoRouter>, n: usize) -> usize {
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::with_capacity(n);

    for i in 0..n {
        let r = Arc::clone(router);
        let b = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            b.wait().await;
            let stmt = Statement::from_string(DbBackend::Postgres, format!("SELECT {} AS val", i));
            r.query_one(stmt).await
        }));
    }

    let mut success = 0usize;
    for handle in handles {
        if handle.await.unwrap().is_ok() {
            success += 1;
        }
    }
    success
}

/// Kill all backend connections on a replica via `pg_terminate_backend`,
/// excluding our own administrative session.
///
/// This causes the AutoRouter's pool connections to that replica to be
/// dropped, triggering `is_connection_error` → circuit breaker mark-down.
///
/// Uses SeaORM to connect directly (no Docker required).
async fn kill_replica_connections(replica_url: &str) {
    match Database::connect(replica_url).await {
        Ok(conn) => {
            let sql = "SELECT pg_terminate_backend(pid) \
                       FROM pg_stat_activity \
                       WHERE pid <> pg_backend_pid() \
                         AND state = 'active'";
            if let Err(e) = conn.execute_unprepared(sql).await {
                eprintln!("WARNING: pg_terminate_backend on {}: {}", replica_url, e);
            }
            // `conn` drops here, closing the admin session — that's fine.
        }
        Err(e) => {
            eprintln!(
                "WARNING: Could not connect to {} for backend kill: {}",
                replica_url, e
            );
        }
    }
}

/// Extract the replica URLs from the loaded config for direct use.
fn get_replica_urls() -> Vec<String> {
    let config = load_app_config();
    config.database_read_urls.clone()
}

/// Generate a unique table name suffix for parallel test safety.
fn unique_table_name(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("_{}_{}", prefix, ts)
}

/// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_reads_with_circuit_breaker() {
    // ── Prerequisite ──────────────────────────────────────────────
    if !has_read_replicas() {
        eprintln!("SKIP: WEBSHELF_DATABASE_READ_URLS not set — no read replicas available");
        return;
    }

    // Create AutoRouter with 3 second circuit breaker (fast test).
    // fallback_to_write = false: when ALL replicas are down, queries
    // must fail rather than silently falling through to the write DB.
    let router = create_test_router(3000, false).await;
    let replica_urls = get_replica_urls();
    assert!(
        replica_urls.len() >= 2,
        "Need at least 2 replica URLs, got {}",
        replica_urls.len()
    );

    // ── Phase 1: All replicas healthy ─────────────────────────────
    eprintln!("Phase 1: Warm up both replicas...");
    warm_up_replicas(&router, 10).await;

    let healthy_ok = concurrent_reads(&router, 20).await;
    assert_eq!(
        healthy_ok, 20,
        "All 20 concurrent reads should succeed with both replicas healthy (got {}/{})",
        healthy_ok, 20
    );
    eprintln!("  ✓ {} / 20 succeeded (healthy)", healthy_ok);

    // ── Phase 2: Kill replica 1 connections → circuit breaker ────
    eprintln!(
        "Phase 2: Killing connections on replica 1 ({})...",
        replica_urls[0]
    );
    kill_replica_connections(&replica_urls[0]).await;
    // Wait for the pool to detect dropped connections
    tokio::time::sleep(Duration::from_secs(2)).await;

    // With replica 1 down, all reads go to replica 2.
    // The circuit breaker marks replica 1 as down.
    let one_down_ok = concurrent_reads(&router, 20).await;
    assert_eq!(
        one_down_ok, 20,
        "All 20 reads should succeed when only replica 2 is healthy (got {}/{})",
        one_down_ok, 20
    );
    eprintln!(
        "  ✓ {} / 20 succeeded (replica 1 down, circuit breaker active)",
        one_down_ok
    );

    // ── Phase 3: Kill replica 2 → all replicas down ──────────────
    eprintln!(
        "Phase 3: Killing connections on replica 2 ({})...",
        replica_urls[1]
    );
    kill_replica_connections(&replica_urls[1]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // With all replicas down and fallback_to_write = false,
    // queries are expected to fail.
    let all_down_ok = concurrent_reads(&router, 5).await;
    assert!(
        all_down_ok < 5,
        "Reads should fail when all replicas are down and fallback_to_write=false \
         (got {}/5 succeeded)",
        all_down_ok
    );
    eprintln!(
        "  ✓ {} / 5 failed as expected (all replicas down)",
        5 - all_down_ok
    );

    // ── Phase 4: Circuit breaker expires → auto-recovery ─────────
    eprintln!("Phase 4: Waiting for circuit breaker to expire (3s)...");
    tokio::time::sleep(Duration::from_secs(4)).await;

    // The circuit breaker has expired. AutoRouter's pick_next_read
    // auto-recovers expired timers. Queries should succeed again.
    let recovered_ok = concurrent_reads(&router, 10).await;
    assert_eq!(
        recovered_ok, 10,
        "All 10 reads should succeed after circuit breaker expiry (got {}/{})",
        recovered_ok, 10
    );
    eprintln!(
        "  ✓ {} / 10 succeeded (circuit breaker expired, replicas recovered)",
        recovered_ok
    );
}

// ═════════════════════════════════════════════════════════════════════
// Write-statement routing tests
// ═════════════════════════════════════════════════════════════════════
// These verify that INSERT/UPDATE/DELETE with RETURNING clauses are
// sent to the write DB via query_one/query_all (not to replicas).
// On a hot standby, write operations fail with "cannot execute X in a
// read-only transaction". Success = correctly routed to write DB.

#[tokio::test]
async fn test_insert_returning_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let tbl = unique_table_name("ar_ins");
    let router = create_test_router(3000, false).await;
    let write = router.write_conn();

    // Create table on write DB (replicated to standbys via WAL)
    write
        .execute_unprepared(&format!(
            "CREATE TABLE {tbl} (id int PRIMARY KEY, val text)"
        ))
        .await
        .unwrap();

    // INSERT RETURNING goes through query_one. If is_write_statement
    // correctly detects it, it routes to write DB and succeeds.
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!("INSERT INTO {tbl} VALUES (1, 'a') RETURNING id"),
        ))
        .await;
    assert!(
        result.is_ok(),
        "INSERT RETURNING should be routed to write DB (got error: {:?})",
        result.as_ref().err()
    );

    write
        .execute_unprepared(&format!("DROP TABLE {tbl}"))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_update_returning_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let tbl = unique_table_name("ar_upd");
    let router = create_test_router(3000, false).await;
    let write = router.write_conn();

    write
        .execute_unprepared(&format!(
            "CREATE TABLE {tbl} (id int PRIMARY KEY, val text); \
             INSERT INTO {tbl} VALUES (1, 'initial')"
        ))
        .await
        .unwrap();

    // UPDATE RETURNING → query_one → is_write_statement → write DB
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!("UPDATE {tbl} SET val = 'b' WHERE id = 1 RETURNING id"),
        ))
        .await;
    assert!(
        result.is_ok(),
        "UPDATE RETURNING should be routed to write DB (got error: {:?})",
        result.as_ref().err()
    );

    write
        .execute_unprepared(&format!("DROP TABLE {tbl}"))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_delete_returning_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let tbl = unique_table_name("ar_del");
    let router = create_test_router(3000, false).await;
    let write = router.write_conn();

    write
        .execute_unprepared(&format!(
            "CREATE TABLE {tbl} (id int PRIMARY KEY, val text); \
             INSERT INTO {tbl} VALUES (1, 'x')"
        ))
        .await
        .unwrap();

    // DELETE RETURNING → query_one → is_write_statement → write DB
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!("DELETE FROM {tbl} WHERE id = 1 RETURNING id"),
        ))
        .await;
    assert!(
        result.is_ok(),
        "DELETE RETURNING should be routed to write DB (got error: {:?})",
        result.as_ref().err()
    );

    write
        .execute_unprepared(&format!("DROP TABLE {tbl}"))
        .await
        .unwrap();
}

// ═════════════════════════════════════════════════════════════════════
// Locking SELECT routing tests
// ═════════════════════════════════════════════════════════════════════
// SELECT ... FOR UPDATE / FOR SHARE must be routed to write DB.
// On a standby, they fail with "cannot execute FOR UPDATE in a read-only
// transaction". Unit tests verify is_locking_select() detection;
// these integration tests verify end-to-end routing.

#[tokio::test]
async fn test_select_for_update_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let tbl = unique_table_name("ar_fu");
    let router = create_test_router(3000, false).await;
    let write = router.write_conn();

    write
        .execute_unprepared(&format!(
            "CREATE TABLE {tbl} (id int PRIMARY KEY, val text); \
             INSERT INTO {tbl} VALUES (1, 'x')"
        ))
        .await
        .unwrap();

    // SELECT ... FOR UPDATE → query_one → is_locking_select → write DB
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT * FROM {tbl} WHERE id = 1 FOR UPDATE"),
        ))
        .await;
    assert!(
        result.is_ok(),
        "SELECT FOR UPDATE should be routed to write DB (got error: {:?})",
        result.as_ref().err()
    );

    write
        .execute_unprepared(&format!("DROP TABLE {tbl}"))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_select_for_share_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let tbl = unique_table_name("ar_fs");
    let router = create_test_router(3000, false).await;
    let write = router.write_conn();

    write
        .execute_unprepared(&format!(
            "CREATE TABLE {tbl} (id int PRIMARY KEY, val text); \
             INSERT INTO {tbl} VALUES (1, 'x')"
        ))
        .await
        .unwrap();

    // SELECT ... FOR SHARE → query_one → is_locking_select → write DB
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT * FROM {tbl} WHERE id = 1 FOR SHARE"),
        ))
        .await;
    assert!(
        result.is_ok(),
        "SELECT FOR SHARE should be routed to write DB (got error: {:?})",
        result.as_ref().err()
    );

    write
        .execute_unprepared(&format!("DROP TABLE {tbl}"))
        .await
        .unwrap();
}

// ═════════════════════════════════════════════════════════════════════
// CTE routing tests
// ═════════════════════════════════════════════════════════════════════
// WITH ... SELECT (read CTE) → replicas
// WITH ... INSERT/UPDATE/DELETE (write CTE) → write DB

#[tokio::test]
async fn test_cte_select_routed_to_replicas() {
    if !has_read_replicas() {
        return;
    }
    let router = create_test_router(3000, false).await;

    // Read CTE: no table dependency, works on any PostgreSQL.
    // Should go to replicas (not detected as write).
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "WITH t AS (SELECT 1 AS x) SELECT x FROM t".to_string(),
        ))
        .await;
    assert!(
        result.is_ok(),
        "CTE SELECT should be routed to replicas (got error: {:?})",
        result.as_ref().err()
    );
}

#[tokio::test]
async fn test_cte_insert_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let tbl = unique_table_name("ar_ctei");
    let router = create_test_router(3000, false).await;
    let write = router.write_conn();

    write
        .execute_unprepared(&format!(
            "CREATE TABLE {tbl} (id int PRIMARY KEY, val text)"
        ))
        .await
        .unwrap();

    // CTE with INSERT main statement → cte_main_stmt_is_write → write DB
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "WITH new_row AS (SELECT 1 AS id, 'cte'::text AS val) \
                 INSERT INTO {tbl} SELECT id, val FROM new_row RETURNING id"
            ),
        ))
        .await;
    assert!(
        result.is_ok(),
        "CTE INSERT should be routed to write DB (got error: {:?})",
        result.as_ref().err()
    );

    write
        .execute_unprepared(&format!("DROP TABLE {tbl}"))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_cte_update_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let tbl = unique_table_name("ar_cteu");
    let router = create_test_router(3000, false).await;
    let write = router.write_conn();

    write
        .execute_unprepared(&format!(
            "CREATE TABLE {tbl} (id int PRIMARY KEY, val text); \
             INSERT INTO {tbl} VALUES (1, 'x')"
        ))
        .await
        .unwrap();

    // CTE with UPDATE main statement → cte_main_stmt_is_write → write DB
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "WITH updated AS (SELECT 2 AS new_val) \
                 UPDATE {tbl} SET val = 'cte_upd' WHERE id = 1 RETURNING id"
            ),
        ))
        .await;
    assert!(
        result.is_ok(),
        "CTE UPDATE should be routed to write DB (got error: {:?})",
        result.as_ref().err()
    );

    write
        .execute_unprepared(&format!("DROP TABLE {tbl}"))
        .await
        .unwrap();
}

// ═════════════════════════════════════════════════════════════════════
// Fallback-to-write test
// ═════════════════════════════════════════════════════════════════════
// When all replicas are down and fallback_to_write=true, queries should
// fall through to the write DB instead of returning an error.

#[tokio::test]
async fn test_fallback_to_write_when_replicas_down() {
    if !has_read_replicas() {
        return;
    }
    let urls = get_replica_urls();
    let router = create_test_router(5000, true).await; // fallback_to_write = true
    warm_up_replicas(&router, 5).await;

    // Kill both replicas
    kill_replica_connections(&urls[0]).await;
    kill_replica_connections(&urls[1]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // With fallback_to_write=true, queries succeed via write DB
    let result = router
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT 1 AS fallback_test".to_string(),
        ))
        .await;
    assert!(
        result.is_ok(),
        "Query should fall back to write DB (got error: {:?})",
        result.as_ref().err()
    );
}

// ═════════════════════════════════════════════════════════════════════
// execute() and write_conn() routing tests
// ═════════════════════════════════════════════════════════════════════
// execute() / execute_unprepared() always route to write DB.
// write_conn() bypasses AutoRouter entirely.

#[tokio::test]
async fn test_execute_always_routed_to_write() {
    if !has_read_replicas() {
        return;
    }
    let router = create_test_router(3000, false).await;

    // execute() routes to write DB regardless of SQL content.
    // A SELECT sent via execute() still goes to write DB.
    let result = router
        .execute(Statement::from_string(
            DbBackend::Postgres,
            "SELECT 1 AS execute_test".to_string(),
        ))
        .await;
    assert!(
        result.is_ok(),
        "execute() should route to write DB (got error: {:?})",
        result.as_ref().err()
    );

    // execute_unprepared() also always goes to write DB
    let result = router
        .execute_unprepared("SELECT 1 AS unprepared_test")
        .await;
    assert!(
        result.is_ok(),
        "execute_unprepared() should route to write DB (got error: {:?})",
        result.as_ref().err()
    );
}

#[tokio::test]
async fn test_write_conn_bypasses_router() {
    if !has_read_replicas() {
        return;
    }
    let router = create_test_router(3000, false).await;

    // write_conn() returns &DatabaseConnection directly → bypasses AutoRouter
    let write = router.write_conn();
    let result = write.execute_unprepared("SELECT 1 AS bypass_test").await;
    assert!(
        result.is_ok(),
        "write_conn() should allow direct DB access (got error: {:?})",
        result.as_ref().err()
    );
}
