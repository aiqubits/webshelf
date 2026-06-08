use anyhow::{Context, Result};
use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait,
};

/// Check if a database error is an expected "already exists" error for
/// idempotent migration statements (CREATE TABLE IF NOT EXISTS, CREATE INDEX IF NOT EXISTS).
///
/// Uses sea-orm's structured `sql_err()` for UniqueConstraintViolation (PostgreSQL 23505)
/// and falls back to string matching for duplicate_table (42P07) and duplicate_object (42710)
/// which are not covered by sea-orm's SqlErr enum.
fn is_duplicate_table_or_index_error(err: &DbErr) -> bool {
    // Structured matching: UniqueConstraintViolation covers index/constraint duplicates
    if let Some(sql_err) = err.sql_err() {
        match sql_err {
            sea_orm::SqlErr::UniqueConstraintViolation(_) => return true,
            sea_orm::SqlErr::ForeignKeyConstraintViolation(_) => return false,
            _ => {}
        }
    }

    // Fallback string matching for errors not covered by SqlErr:
    // - 42P07: duplicate_table (CREATE TABLE IF NOT EXISTS safety net)
    // - 42710: duplicate_object (CREATE INDEX IF NOT EXISTS safety net)
    let msg = err.to_string().to_lowercase();
    msg.contains("42p07")
        || msg.contains("42710")
        || (msg.contains("already exists") && msg.contains("relation"))
}

/// Run database migrations.
///
/// # Limitations
///
/// **SQL splitting**: Statements are split by `;` using simple string splitting.
/// This means SQL string literals containing semicolons (e.g., `INSERT INTO t
/// VALUES ('hello;world')`) will be incorrectly split. Current migrations are
/// simple enough that this is not triggered, but future migrations must avoid
/// semicolons inside string literals, or the splitting logic should be upgraded
/// to use a proper SQL parser.
///
/// **No migration tracking**: There is no `_migrations` table to track which
/// migrations have been applied. All migrations use idempotent statements
/// (`IF NOT EXISTS`) and are re-executed on every startup. For production use,
/// consider adopting `sea-orm-cli`'s migration framework for proper versioning,
/// rollback support, and incremental application.
pub async fn run_migrations(db: &DatabaseConnection) -> Result<()> {
    tracing::info!("Running database migrations...");

    // Read migration files
    let migrations = vec![(
        "001_create_users_table",
        include_str!("../migrations/001_create_users_table.sql"),
    )];

    for (name, sql) in migrations {
        tracing::info!("Running migration: {}", name);

        // Split by semicolon and execute each statement in its own savepoint
        // to prevent one failing statement from aborting the entire transaction
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Use a savepoint for each statement so that failures don't abort
            // the entire migration transaction (PostgreSQL requires this)
            let savepoint = db.begin().await.context("Failed to begin savepoint")?;

            match savepoint
                .execute(Statement::from_string(
                    DatabaseBackend::Postgres,
                    trimmed.to_string(),
                ))
                .await
            {
                Ok(_) => {
                    savepoint
                        .commit()
                        .await
                        .context("Failed to commit savepoint")?;
                }
                Err(e) => {
                    savepoint
                        .rollback()
                        .await
                        .context("Failed to rollback savepoint")?;

                    // Try structured error matching first via sea-orm's sql_err()
                    let is_expected_error = is_duplicate_table_or_index_error(&e);

                    if is_expected_error {
                        tracing::warn!("Migration statement skipped (already exists): {}", trimmed);
                    } else {
                        return Err(e).context(format!("Failed to execute migration: {}", name));
                    }
                }
            }
        }

        tracing::info!("Migration completed: {}", name);
    }

    tracing::info!("All migrations completed successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_migrations_list() {
        let migrations = [(
            "001_create_users_table",
            include_str!("../migrations/001_create_users_table.sql"),
        )];

        assert_eq!(migrations.len(), 1);
        assert!(migrations[0].1.contains("CREATE TABLE IF NOT EXISTS users"));
    }
}
