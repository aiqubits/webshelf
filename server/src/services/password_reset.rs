use crate::repositories::user::{Column, Entity as UserEntity};
use crate::utils::db_router::AutoRouter;
use crate::utils::password::hash_password;
use anyhow::Context;
use argon2::password_hash::PasswordHash;
use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::{Duration, Utc};
use emailserver::EmailService;
use rand::Rng;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, QueryFilter, Statement,
    TransactionTrait, sea_query::Expr,
};
use std::sync::Arc;

const CODE_EXPIRY_MINUTES: i64 = 10;
const RESEND_COOLDOWN_SECONDS: i64 = 60;
const MAX_FAILED_ATTEMPTS: i32 = 5;

/// Dummy Argon2 hash used for constant-time comparisons when a user does
/// not exist.  Performing an Argon2 verification against this hash costs the
/// same CPU time as verifying a real code, preventing attackers from
/// enumerating registered emails by measuring response times on the
/// forgot-password and reset-password endpoints.
fn dummy_code_hash() -> &'static str {
    static DUMMY_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY_HASH.get_or_init(|| hash_code("000000").expect("Failed to build dummy code hash"))
}

#[derive(Debug, thiserror::Error)]
pub enum PasswordResetError {
    #[error("Invalid or expired reset code")]
    InvalidOrExpired,
    #[error("Too many attempts, please wait or request a new code")]
    TooManyAttempts,
    #[error("Too soon to request another code")]
    TooSoon,
    #[error("Email service not configured")]
    EmailNotConfigured,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Outcome of a successful password reset, used by the handler to issue a
/// fresh JWT so the caller is auto-logged-in after resetting.
pub struct PasswordResetOutcome {
    pub user_id: i64,
    pub role: String,
    pub token_version: i32,
}

fn generate_code() -> String {
    let code = rand::thread_rng().gen_range(0..1_000_000);
    format!("{:06}", code)
}

fn hash_code(code: &str) -> anyhow::Result<String> {
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut rand::thread_rng());
    let hash = argon2
        .hash_password(code.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash reset code: {}", e))?;
    Ok(hash.to_string())
}

fn verify_code(code: &str, hash: &str) -> anyhow::Result<bool> {
    let argon2 = Argon2::default();
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| anyhow::anyhow!("Failed to parse stored reset code hash: {}", e))?;
    Ok(argon2
        .verify_password(code.as_bytes(), &parsed_hash)
        .is_ok())
}

pub struct PasswordResetService {
    db: Arc<AutoRouter>,
    email: EmailService,
}

impl PasswordResetService {
    pub fn new(db: Arc<AutoRouter>, email: EmailService) -> Self {
        Self { db, email }
    }

    async fn find_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<crate::repositories::user::Model>, PasswordResetError> {
        UserEntity::find()
            .filter(Column::Email.eq(email))
            .one(self.db.write_conn())
            .await
            .context("Failed to query user")
            .map_err(Into::into)
    }

    /// Request a password-reset verification code sent to the user's email.
    ///
    /// Anti-enumeration:
    /// - For non-existing users, performs a dummy Argon2 hash so the timing
    ///   matches the existent-user path and returns `Ok(())` — regardless of
    ///   whether the email service is configured.  This prevents attackers
    ///   from inferring user existence via a 503 response.
    /// - When the email service is not configured and the user exists, returns
    ///   `EmailNotConfigured` (mapped to 503) AFTER the user lookup, so the
    ///   503 only surfaces for registered emails.
    /// - For existing users within cooldown, returns `TooSoon`.
    /// - For existing users past cooldown, stores a new code hash and sends
    ///   the verification email.
    pub async fn request_reset(&self, email: &str) -> Result<(), PasswordResetError> {
        let email_normalized = email.to_lowercase();

        // Anti-enumeration: if the user does not exist, perform a dummy
        // Argon2 verify (same CPU cost as the real path) and return Ok(())
        // WITHOUT checking email service configuration first.  This ensures
        // non-existent emails always receive 200 regardless of SMTP state.
        let user = match self.find_user_by_email(&email_normalized).await? {
            Some(u) => u,
            None => {
                let _ = verify_code("000000", dummy_code_hash());
                return Ok(());
            }
        };

        // Only check email service configuration AFTER confirming the user
        // exists.  This prevents leaking user existence information via the
        // 503 response.
        if !self.email.is_configured().await {
            return Err(PasswordResetError::EmailNotConfigured);
        }

        let now = Utc::now();
        let cooldown_threshold = now - Duration::seconds(RESEND_COOLDOWN_SECONDS);

        // Generate the code eagerly so that creation cost is always paid
        // before we open any DB write.
        let code = generate_code();
        let code_hash = hash_code(&code)?;
        let expires_at = now + Duration::minutes(CODE_EXPIRY_MINUTES);

        // ── Atomic cooldown enforcement ──────────────────────────────────
        // Single UPDATE ... WHERE predicate; rows_affected == 0 means the
        // cooldown predicate did not hold. Eliminates the TOCTOU race where
        // two concurrent requests could both observe an old sent_at and both
        // issue a new code.
        let result = UserEntity::update_many()
            .col_expr(Column::PasswordResetTokenHash, Expr::value(code_hash))
            .col_expr(Column::PasswordResetExpiresAt, Expr::value(expires_at))
            .col_expr(Column::PasswordResetSentAt, Expr::value(now))
            .col_expr(Column::PasswordResetFailedAttempts, Expr::value(0))
            .filter(Column::Id.eq(user.id))
            .filter(
                Column::PasswordResetSentAt
                    .is_null()
                    .or(Column::PasswordResetSentAt.lte(cooldown_threshold)),
            )
            .exec(&*self.db)
            .await
            .context("Failed to store password-reset code")?;

        if result.rows_affected == 0 {
            return Err(PasswordResetError::TooSoon);
        }

        // Send synchronously so the caller learns about failures immediately.
        self.email
            .send_password_reset_code_email(&email_normalized, &code, CODE_EXPIRY_MINUTES)
            .await
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send password-reset code email to {}: {:?}",
                    email_normalized,
                    e
                );
            })
            .map_err(|e| {
                PasswordResetError::Internal(anyhow::anyhow!(
                    "Failed to send password-reset code email: {}",
                    e
                ))
            })?;

        tracing::info!("Password-reset code sent to {}", email_normalized);
        Ok(())
    }

    /// Consume a password-reset verification code and replace the user's
    /// password.
    ///
    /// Anti-enumeration: for non-existent users, performs a dummy Argon2
    /// verification so the timing profile matches the existent-user path.
    ///
    /// Brute-force protection: the failed-attempts counter is incremented
    /// atomically via `UPDATE … WHERE failed_attempts < MAX`.
    ///
    /// Atomicity: the password update and the `token_version` increment are
    /// wrapped in a single transaction.
    pub async fn reset_password(
        &self,
        email: &str,
        code: &str,
        new_password: &str,
    ) -> Result<PasswordResetOutcome, PasswordResetError> {
        let email_normalized = email.to_lowercase();

        // Anti-enumeration: non-existent users get a dummy Argon2 verify so
        // the timing profile matches the existent-user path.
        let user = match self.find_user_by_email(&email_normalized).await? {
            Some(u) => u,
            None => {
                let _ = verify_code(code, dummy_code_hash());
                return Err(PasswordResetError::InvalidOrExpired);
            }
        };

        let stored_hash = match user.password_reset_token_hash.as_deref() {
            Some(h) => h.to_string(),
            None => {
                let _ = verify_code(code, dummy_code_hash());
                return Err(PasswordResetError::InvalidOrExpired);
            }
        };

        let expires_at = match user.password_reset_expires_at {
            Some(t) => t,
            None => {
                // Constant-time: match the non-existent-user path
                let _ = verify_code(code, dummy_code_hash());
                return Err(PasswordResetError::InvalidOrExpired);
            }
        };

        if Utc::now() > expires_at {
            // Constant-time: match the non-existent-user path
            let _ = verify_code(code, dummy_code_hash());
            return Err(PasswordResetError::InvalidOrExpired);
        }

        // Atomic claim: increment the counter only if the user is below
        // the threshold. Runs OUTSIDE the transaction so that the counter
        // persists even on wrong-code or concurrent-consumption detection.
        self.increment_failed_attempts(user.id).await?;

        // Verify code against stored hash
        if !verify_code(code, &stored_hash)? {
            return Err(PasswordResetError::InvalidOrExpired);
        }

        let new_hash = hash_password(new_password).context("Failed to hash new password")?;

        // ── Atomic claim + update ────────────────────────────────────
        // A single UPDATE with a WHERE guard prevents the TOCTOU race
        // where two concurrent requests both pass the Argon2 verification
        // and try to consume the same reset code. The
        // `WHERE password_reset_token_hash IS NOT NULL` condition ensures
        // the first request to commit wins; the second sees 0 rows
        // affected and returns InvalidOrExpired.
        //
        // This also atomically:
        // - Increments token_version (invalidating all existing JWTs)
        // - Replaces the password hash
        // - Clears the single-use reset code fields
        let txn = self
            .db
            .begin()
            .await
            .context("Failed to begin transaction for password reset")?;

        let result = txn
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"UPDATE users SET
                    token_version = token_version + 1,
                    password_hash = $2,
                    updated_at = NOW(),
                    password_reset_token_hash = NULL,
                    password_reset_expires_at = NULL,
                    password_reset_sent_at = NULL,
                    password_reset_failed_attempts = 0
                   WHERE id = $1 AND password_reset_token_hash IS NOT NULL"#,
                [user.id.into(), new_hash.into()],
            ))
            .await
            .context("Failed to atomically update password and consume reset code")?;

        if result.rows_affected() == 0 {
            txn.rollback()
                .await
                .context("Failed to rollback after detecting concurrent reset-code consumption")?;
            return Err(PasswordResetError::InvalidOrExpired);
        }

        // Re-fetch to obtain the post-update token_version for JWT signing
        let updated = UserEntity::find()
            .filter(Column::Id.eq(user.id))
            .one(&txn)
            .await
            .context("Failed to re-fetch user after password reset")?
            .ok_or_else(|| anyhow::anyhow!("User vanished after password-reset UPDATE"))?;

        // Revoke all refresh tokens so a stolen refresh cookie cannot mint
        // a fresh JWT at the new token_version. Without this, the
        // token_version bump alone is insufficient — the refresh endpoint
        // reads the current user.token_version and would happily sign a new
        // JWT for an attacker holding a still-valid refresh cookie.
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM refresh_tokens WHERE user_id = $1",
            [updated.id.into()],
        ))
        .await
        .context("Failed to revoke refresh tokens during password reset")?;

        txn.commit()
            .await
            .context("Failed to commit password-reset transaction")?;

        tracing::info!("Password reset succeeded for user {}", updated.id);
        Ok(PasswordResetOutcome {
            user_id: updated.id,
            role: updated.role,
            token_version: updated.token_version,
        })
    }

    /// Atomically increment the failed-attempts counter, enforcing the
    /// brute-force threshold via `UPDATE … WHERE failed_attempts < MAX`.
    async fn increment_failed_attempts(&self, user_id: i64) -> Result<(), PasswordResetError> {
        let result = UserEntity::update_many()
            .col_expr(
                Column::PasswordResetFailedAttempts,
                sea_orm::sea_query::Expr::col(Column::PasswordResetFailedAttempts).add(1),
            )
            .filter(Column::Id.eq(user_id))
            .filter(Column::PasswordResetFailedAttempts.lt(MAX_FAILED_ATTEMPTS))
            .exec(&*self.db)
            .await
            .context("Failed to increment reset failed attempts")?;

        if result.rows_affected == 0 {
            // Constant-time: match the non-existent-user path
            let _ = verify_code("000000", dummy_code_hash());
            return Err(PasswordResetError::TooManyAttempts);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_is_six_digits() {
        for _ in 0..100 {
            let code = generate_code();
            assert_eq!(code.len(), 6, "Code must be 6 characters");
            assert!(
                code.chars().all(|c| c.is_ascii_digit()),
                "Code must be numeric"
            );
        }
    }

    #[test]
    fn test_hash_and_verify_code() {
        let code = "123456";
        let hash = hash_code(code).unwrap();
        assert!(verify_code(code, &hash).unwrap());
        assert!(!verify_code("654321", &hash).unwrap());
    }

    #[test]
    fn test_different_codes_produce_different_hashes() {
        let hash1 = hash_code("111111").unwrap();
        let hash2 = hash_code("222222").unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_same_code_produces_different_hashes_due_to_salt() {
        let hash1 = hash_code("123456").unwrap();
        let hash2 = hash_code("123456").unwrap();
        assert_ne!(hash1, hash2);
        assert!(verify_code("123456", &hash1).unwrap());
        assert!(verify_code("123456", &hash2).unwrap());
    }

    #[test]
    fn test_dummy_code_hash_is_stable() {
        let a = dummy_code_hash();
        let b = dummy_code_hash();
        assert_eq!(a, b);
        // Sanity: the dummy hash is a valid Argon2 PHC string.
        assert!(PasswordHash::new(a).is_ok());
    }

    #[test]
    fn test_password_reset_error_display() {
        let e = PasswordResetError::InvalidOrExpired;
        assert_eq!(e.to_string(), "Invalid or expired reset code");
        let e = PasswordResetError::TooManyAttempts;
        assert!(e.to_string().contains("Too many attempts"));
        let e = PasswordResetError::TooSoon;
        assert!(e.to_string().contains("Too soon"));
        let e = PasswordResetError::EmailNotConfigured;
        assert!(e.to_string().contains("not configured"));
    }
}
