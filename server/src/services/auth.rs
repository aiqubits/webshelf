use crate::repositories::user::Entity as UserEntity;
use crate::utils::db_router::AutoRouter;
use crate::utils::jwt::generate_token;
use crate::utils::password::{hash_password, verify_password};
use anyhow::Context;
use rand::RngCore;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Typed errors for authentication operations
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid email or password")]
    InvalidCredentials,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Authentication service for user login and token management
pub struct AuthService {
    db: Arc<AutoRouter>,
    jwt_secret: String,
    jwt_expiry_seconds: u64,
    jwt_remember_expiry_seconds: u64,
    refresh_token_expiry_seconds: u64,
}

/// Login request payload
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub remember: bool,
}

/// Login response with token
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
    #[serde(skip_serializing)]
    pub refresh_token: String,
    pub refresh_expires_in: u64,
}

impl AuthService {
    /// Create a new authentication service
    pub fn new(
        db: Arc<AutoRouter>,
        jwt_secret: String,
        jwt_expiry_seconds: u64,
        jwt_remember_expiry_seconds: u64,
        refresh_token_expiry_seconds: u64,
    ) -> Self {
        Self {
            db,
            jwt_secret,
            jwt_expiry_seconds,
            jwt_remember_expiry_seconds,
            refresh_token_expiry_seconds,
        }
    }

    /// Authenticate user with email and password.
    ///
    /// Uses constant-time comparison: always performs an Argon2 operation
    /// regardless of whether the user exists, to prevent timing-based email
    /// enumeration attacks.
    ///
    /// When `remember` is true, the JWT expiry is extended to
    /// `jwt_remember_expiry_seconds` (default 30 days) instead of the
    /// standard `jwt_expiry_seconds` (default 1 hour).
    pub async fn login(&self, request: LoginRequest) -> Result<LoginResponse, AuthError> {
        let email_normalized = request.email.to_lowercase();
        let user_result = UserEntity::find()
            .filter(crate::repositories::user::Column::Email.eq(&email_normalized))
            .one(self.db.write_conn())
            .await
            .context("Failed to query user")?;

        // Constant-time: always perform an Argon2 operation regardless of
        // whether the user exists, to prevent timing-based user enumeration.
        // For non-existent users, we hash the provided password (same cost
        // as verification) and discard the result.
        let (user, is_valid) = match user_result {
            Some(user) => {
                let is_valid = verify_password(&request.password, &user.password_hash)
                    .context("Failed to verify password")?;
                (Some(user), is_valid)
            }
            None => {
                // Hash the provided password to spend equivalent CPU time.
                // The result is intentionally discarded — this is purely to
                // make the non-existent-user path as slow as the existent-user
                // path, preventing attackers from enumerating valid emails
                // by measuring response time.
                if let Err(e) = hash_password(&request.password) {
                    tracing::warn!(
                        "Honeypot password hash failed for non-existent user: {:?}",
                        e
                    );
                }
                (None, false)
            }
        };

        let user = user.ok_or(AuthError::InvalidCredentials)?;

        if !is_valid {
            return Err(AuthError::InvalidCredentials);
        }

        if !user.email_verified {
            // Return the same error as invalid credentials to prevent
            // user enumeration — an attacker should not be able to
            // distinguish "wrong password" from "email not verified".
            tracing::info!("Login rejected for {}: email not verified", user.email);
            return Err(AuthError::InvalidCredentials);
        }

        let jwt_expiry = if request.remember {
            self.jwt_remember_expiry_seconds
        } else {
            self.jwt_expiry_seconds
        };

        let token = generate_token(
            &user.id.to_string(),
            &user.role,
            &self.jwt_secret,
            jwt_expiry,
            request.remember,
            user.token_version,
        )
        .map_err(|e| {
            tracing::error!("Failed to generate JWT for user {}: {:?}", user.id, e);
            anyhow::anyhow!("Failed to generate token: {}", e)
        })?;

        tracing::info!("JWT generated successfully for user {}", user.id);

        // Refresh tokens are only issued for "remember me" sessions. A
        // non-remembered login is a transient session that ends when the
        // JWT itself expires — issuing a 90-day refresh token in that case
        // would be a privilege escalation (a 1-hour-session user could keep
        // themselves logged in for months via the refresh endpoint). The
        // empty-string + zero-expires signals to the handler "do not set
        // a refresh cookie" without requiring a separate response variant.
        let (raw_refresh_token, refresh_expires_in) = if request.remember {
            let (raw, hash) = Self::generate_refresh_token();
            tracing::info!("Refresh token generated for user {}", user.id);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("Failed to get current time")?;
            let refresh_expires_at = chrono::DateTime::from_timestamp(
                (now.as_secs() + self.refresh_token_expiry_seconds) as i64,
                0,
            )
            .context("Failed to compute refresh token expiry")?;

            // Delete any existing refresh tokens for this user before storing
            // the new one.  This ensures a user has at most one active
            // refresh token per login, preventing token accumulation across
            // repeated logins on the same device.
            //
            // Both operations run within a single transaction so that a
            // partial failure (e.g., delete succeeds but insert fails) does
            // not leave the user without any refresh token.
            let txn = self
                .db
                .begin()
                .await
                .context("Failed to begin transaction for refresh token rotation")?;

            self.delete_all_refresh_tokens(&txn, user.id).await?;
            self.store_refresh_token(&txn, user.id, &hash, refresh_expires_at)
                .await?;

            txn.commit()
                .await
                .context("Failed to commit refresh token rotation")?;

            (raw, self.refresh_token_expiry_seconds)
        } else {
            (String::new(), 0)
        };

        tracing::info!(
            "User {} logged in successfully (remember={})",
            user.id,
            request.remember
        );

        Ok(LoginResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: jwt_expiry,
            user_id: user.id.to_string(),
            role: user.role,
            refresh_token: raw_refresh_token,
            refresh_expires_in,
        })
    }

    /// Generate a cryptographically random refresh token and its SHA-256 hash.
    ///
    /// Returns `(raw_token, token_hash)`. The raw token is sent to the client
    /// (stored in an httpOnly cookie); only the hash is persisted in the DB.
    pub fn generate_refresh_token() -> (String, String) {
        let mut rng = rand::thread_rng();
        let mut bytes = vec![0u8; 48];
        rng.fill_bytes(&mut bytes);
        let raw = hex::encode(&bytes);
        let hash = hex::encode(Sha256::digest(raw.as_bytes()));
        (raw, hash)
    }

    /// Store a refresh token hash in the database via the given connection.
    ///
    /// Accepts a generic connection parameter so the operation can be part of
    /// a transaction (pass `&txn`) or run standalone (pass `&self.db`).
    pub async fn store_refresh_token(
        &self,
        db: &impl ConnectionTrait,
        user_id: i64,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AuthError> {
        use crate::repositories::refresh_token::{ActiveModel, Entity as RefreshTokenEntity};
        use sea_orm::ActiveValue::NotSet;

        let model = ActiveModel {
            id: NotSet, // Let the database auto-generate the BIGSERIAL primary key
            user_id: Set(user_id),
            token_hash: Set(token_hash.to_string()),
            expires_at: Set(expires_at),
            created_at: Set(chrono::Utc::now()),
        };

        RefreshTokenEntity::insert(model)
            .exec(db)
            .await
            .map_err(|e| {
                tracing::error!(
                    "Failed to store refresh token for user {}: {:?}",
                    user_id,
                    e
                );
                AuthError::Internal(anyhow::anyhow!("Failed to store refresh token: {}", e))
            })?;

        Ok(())
    }

    /// Delete a specific refresh token by its hash via the given connection.
    pub async fn delete_refresh_token(
        &self,
        db: &impl ConnectionTrait,
        token_hash: &str,
    ) -> Result<(), AuthError> {
        use crate::repositories::refresh_token::Entity as RefreshTokenEntity;
        use sea_orm::ColumnTrait;

        RefreshTokenEntity::delete_many()
            .filter(crate::repositories::refresh_token::Column::TokenHash.eq(token_hash))
            .exec(db)
            .await
            .context("Failed to delete refresh token")?;

        Ok(())
    }

    /// Delete all refresh tokens for a user via the given connection.
    pub async fn delete_all_refresh_tokens(
        &self,
        db: &impl ConnectionTrait,
        user_id: i64,
    ) -> Result<(), AuthError> {
        use crate::repositories::refresh_token::Entity as RefreshTokenEntity;
        use sea_orm::ColumnTrait;

        RefreshTokenEntity::delete_many()
            .filter(crate::repositories::refresh_token::Column::UserId.eq(user_id))
            .exec(db)
            .await
            .context("Failed to delete all refresh tokens for user")?;

        Ok(())
    }

    /// Revoke all sessions for a user atomically.
    ///
    /// Deletes all refresh tokens AND increments `token_version` in a single
    /// transaction. This ensures that a partial failure cannot leave the user
    /// in an inconsistent state (e.g., refresh tokens deleted but old JWTs
    /// still valid, or token_version bumped but stale refresh tokens remain).
    ///
    /// Used by `logout_all` to simultaneously invalidate:
    /// - Existing JWTs (via token_version increment)
    /// - Existing refresh tokens (via DELETE)
    pub async fn revoke_all_sessions(&self, user_id: i64) -> Result<(), AuthError> {
        use crate::repositories::refresh_token::Entity as RefreshTokenEntity;
        use sea_orm::{ColumnTrait, ConnectionTrait, DatabaseBackend, Statement, TransactionTrait};

        let txn = self
            .db
            .begin()
            .await
            .context("Failed to begin transaction for revoke_all_sessions")?;

        // 1. Delete all refresh tokens for this user
        RefreshTokenEntity::delete_many()
            .filter(crate::repositories::refresh_token::Column::UserId.eq(user_id))
            .exec(&txn)
            .await
            .context("Failed to delete refresh tokens")?;

        // 2. Atomically increment token_version to invalidate all existing JWTs
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE users SET token_version = token_version + 1 WHERE id = $1",
            [user_id.into()],
        ))
        .await
        .context("Failed to increment token_version")?;

        txn.commit()
            .await
            .context("Failed to commit revoke_all_sessions transaction")?;

        Ok(())
    }

    /// Atomically rotate a refresh token: validate the old one, delete it,
    /// and store a new one — all within a single transaction.
    ///
    /// Returns `(user_id, role, token_version)` on success.
    /// Returns `None` if the old token is invalid, expired, or was already
    /// consumed by a concurrent request.
    pub async fn rotate_refresh_token(
        &self,
        old_token_hash: &str,
        new_token_hash: &str,
        new_expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<(i64, String, i32)>, AuthError> {
        use crate::repositories::refresh_token::{
            ActiveModel, Column, Entity as RefreshTokenEntity, Model,
        };
        use sea_orm::{
            ActiveValue::NotSet, ColumnTrait, QueryFilter, QuerySelect, TransactionTrait,
        };

        let txn = self
            .db
            .begin()
            .await
            .context("Failed to begin transaction")?;

        let now = chrono::Utc::now();

        let token_record: Option<Model> = RefreshTokenEntity::find()
            .filter(Column::TokenHash.eq(old_token_hash))
            .filter(Column::ExpiresAt.gt(now))
            .lock_exclusive()
            .one(&txn)
            .await
            .context("Failed to query refresh token")?;

        let token_record = match token_record {
            Some(r) => r,
            None => return Ok(None),
        };

        let user = UserEntity::find_by_id(token_record.user_id)
            .one(&txn)
            .await
            .context("Failed to query user")?;

        let user = match user {
            Some(u) => u,
            None => return Ok(None),
        };

        RefreshTokenEntity::delete_many()
            .filter(Column::TokenHash.eq(old_token_hash))
            .exec(&txn)
            .await
            .context("Failed to delete old refresh token")?;

        let model = ActiveModel {
            id: NotSet,
            user_id: Set(user.id),
            token_hash: Set(new_token_hash.to_string()),
            expires_at: Set(new_expires_at),
            created_at: Set(chrono::Utc::now()),
        };

        RefreshTokenEntity::insert(model)
            .exec(&txn)
            .await
            .context("Failed to store new refresh token")?;

        txn.commit().await.context("Failed to commit transaction")?;

        Ok(Some((user.id, user.role, user.token_version)))
    }
}

/// Delete all expired refresh tokens from the database.
///
/// Called once during server startup to prevent accumulation of stale rows.
/// Expired rows are never queried (all queries filter `expires_at > now()`),
/// so cleanup is purely an operational concern to limit table bloat.
pub async fn cleanup_expired_refresh_tokens(db: &DatabaseConnection) -> Result<u64, AuthError> {
    use crate::repositories::refresh_token::{Column, Entity as RefreshTokenEntity};

    let now = chrono::Utc::now();
    let result = RefreshTokenEntity::delete_many()
        .filter(Column::ExpiresAt.lte(now))
        .exec(db)
        .await
        .context("Failed to cleanup expired refresh tokens")?;

    let deleted = result.rows_affected;
    if deleted > 0 {
        tracing::info!("Cleaned up {} expired refresh tokens", deleted);
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_request_deserialization() {
        let json = r#"{
            "email": "user@example.com",
            "password": "password123"
        }"#;

        let request: LoginRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.email, "user@example.com");
        assert_eq!(request.password, "password123");
    }

    #[test]
    fn test_login_response_serialization() {
        let response = LoginResponse {
            token: "jwt.token.here".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            user_id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            role: "user".to_string(),
            refresh_token: "refresh.token.here".to_string(),
            refresh_expires_in: 7776000,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("jwt.token.here"));
        assert!(json.contains("Bearer"));
        assert!(json.contains("3600"));
        assert!(json.contains("user"));
    }
}
