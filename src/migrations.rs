use anyhow::{Context, Result};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};

/// Run database migrations
pub async fn run_migrations(db: &DatabaseConnection) -> Result<()> {
    tracing::info!("Running database migrations...");

    // Read migration files
    let migrations = vec![
        ("001_create_users_table", include_str!("../migrations/001_create_users_table.sql")),
    ];

    // Execute each migration
    for (name, sql) in migrations {
        tracing::info!("Running migration: {}", name);
        
        // Split by semicolon and execute each statement
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if !trimmed.is_empty() {
                if let Err(e) = db.execute(Statement::from_string(DatabaseBackend::Postgres, trimmed.to_string())).await {
                    // Log warning but continue if it's a duplicate object error (table/index already exists)
                    let error_msg = e.to_string().to_lowercase();
                    if error_msg.contains("duplicate") || error_msg.contains("already exists") || error_msg.contains("already exists") {
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
        let migrations = vec![
            ("001_create_users_table", include_str!("../migrations/001_create_users_table.sql")),
        ];
        
        assert_eq!(migrations.len(), 1);
        assert!(migrations[0].1.contains("CREATE TABLE IF NOT EXISTS users"));
    }
}