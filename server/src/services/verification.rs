use crate::repositories::user::{ActiveModel, Column, Entity as UserEntity};
use anyhow::Context;
use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::{Duration, Utc};
use emailserver::EmailService;
use rand::Rng;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    sea_query::Expr,
};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

const CODE_EXPIRY_MINUTES: i64 = 10;
const RESEND_COOLDOWN_SECONDS: i64 = 60;
const MAX_FAILED_ATTEMPTS: i32 = 5;
const MAX_CONCURRENT_EMAIL_SENDS: usize = 10;

/// Dummy Argon2 hash used for constant-time comparisons when a user
/// does not exist.  Performing an Argon2 verification against this
/// hash costs the same CPU time as verifying a real code, preventing
/// attackers from enumerating registered emails by measuring response
/// times on the verify-email and resend-code endpoints.
fn dummy_code_hash() -> &'static str {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    DUMMY_HASH.get_or_init(|| VerificationService::hash_code("000000").unwrap())
}

/// Global shared semaphore to limit concurrent email sends across all requests.
fn global_email_send_limiter() -> &'static Arc<Semaphore> {
    static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMITER.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_EMAIL_SENDS)))
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Invalid or expired verification code")]
    InvalidOrExpired,
    #[error("Too many attempts, please wait before trying again")]
    TooManyAttempts,
    #[error("Too soon to resend")]
    TooSoon,
    #[error("Email service not configured")]
    EmailNotConfigured,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub struct VerificationService {
    db: DatabaseConnection,
    email: EmailService,
    email_send_limiter: Arc<Semaphore>,
}

impl VerificationService {
    pub fn new(db: DatabaseConnection, email: EmailService) -> Self {
        Self {
            db,
            email,
            email_send_limiter: global_email_send_limiter().clone(),
        }
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
            .map_err(|e| anyhow::anyhow!("Failed to hash verification code: {}", e))?;
        Ok(hash.to_string())
    }

    fn verify_code(code: &str, hash: &str) -> anyhow::Result<bool> {
        use argon2::password_hash::PasswordHash;
        let argon2 = Argon2::default();
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| anyhow::anyhow!("Failed to parse stored code hash: {}", e))?;
        Ok(argon2
            .verify_password(code.as_bytes(), &parsed_hash)
            .is_ok())
    }

    async fn find_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<crate::repositories::user::Model>, VerificationError> {
        UserEntity::find()
            .filter(Column::Email.eq(email))
            .one(&self.db)
            .await
            .context("Failed to query user")
            .map_err(Into::into)
    }

    async fn store_verification_code(
        &self,
        user: crate::repositories::user::Model,
    ) -> Result<(crate::repositories::user::Model, String), VerificationError> {
        let code = Self::generate_code();
        let code_hash = Self::hash_code(&code)?;
        let now = Utc::now();
        let expires_at = now + Duration::minutes(CODE_EXPIRY_MINUTES);

        let mut active_model: ActiveModel = user.into();
        active_model.verification_code_hash = Set(Some(code_hash));
        active_model.verification_code_expires_at = Set(Some(expires_at));
        active_model.verification_code_sent_at = Set(Some(now));
        // Reset failed attempts when issuing a new code.
        // The 60-second RESEND_COOLDOWN is the primary rate limiter;
        // resetting here gives legitimate users a fresh set of 5 attempts
        // per new code, while the attacker still gets at most ~5 attempts
        // per minute — negligible for a 6-digit (1M combos) search space.
        active_model.verification_failed_attempts = Set(0);
        active_model.updated_at = Set(now);
        let updated = active_model
            .update(&self.db)
            .await
            .context("Failed to store verification code")?;

        Ok((updated, code))
    }

    fn send_welcome_email_background(&self, email: &str, name: &str) {
        let email = email.to_string();
        let name = name.to_string();
        let email_service = self.email.clone();
        let limiter = self.email_send_limiter.clone();
        tokio::spawn(async move {
            let _permit = match limiter.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Email send semaphore closed: {:?}", e);
                    return;
                }
            };
            let mut attempts = 0;
            let max_attempts = 2;
            while attempts < max_attempts {
                match email_service.send_welcome_email(&email, Some(&name)).await {
                    Ok(()) => {
                        tracing::info!("Welcome email sent to {}", email);
                        return;
                    }
                    Err(e) => {
                        attempts += 1;
                        if attempts < max_attempts {
                            tracing::warn!(
                                "Failed to send welcome email to {} (attempt {}/{}): {:?}",
                                email,
                                attempts,
                                max_attempts,
                                e
                            );
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        } else {
                            tracing::warn!(
                                "Failed to send welcome email to {} after {} attempts: {:?}",
                                email,
                                max_attempts,
                                e
                            );
                        }
                    }
                }
            }
        });
    }

    pub async fn send_verification_email(&self, email: &str) -> Result<(), VerificationError> {
        let email_normalized = email.to_lowercase();
        let user = match self.find_user_by_email(&email_normalized).await? {
            Some(u) => u,
            None => return Ok(()),
        };

        // Check email service configuration AFTER user lookup so that
        // non-existent emails always receive 200 regardless of SMTP state,
        // preventing user-enumeration via the 503 response.
        if !self.email.is_configured().await {
            return Err(VerificationError::EmailNotConfigured);
        }

        if user.email_verified {
            return Ok(());
        }

        let (_updated, code) = self.store_verification_code(user).await?;

        // Send synchronously so the caller can handle failures.
        let _permit = self.email_send_limiter.acquire().await.map_err(|e| {
            VerificationError::Internal(anyhow::anyhow!("Email send semaphore closed: {:?}", e))
        })?;
        self.email
            .send_registration_code_email(&email_normalized, &code, CODE_EXPIRY_MINUTES)
            .await
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send verification email to {}: {:?}",
                    email_normalized,
                    e
                );
            })
            .map_err(|e| {
                VerificationError::Internal(anyhow::anyhow!(
                    "Failed to send verification email: {}",
                    e
                ))
            })?;

        tracing::info!("Verification code sent to {}", email_normalized);
        Ok(())
    }

    /// Mark a user's email as verified.
    ///
    /// This is used in two scenarios:
    /// 1. When the email service is not configured (dev/test environments), as a
    ///    graceful degradation path so registration succeeds without SMTP.
    /// 2. As a fallback when the SMTP service is configured but the send fails
    ///    (transient network errors), to avoid orphan accounts that can neither
    ///    log in nor re-register.
    pub async fn auto_verify(&self, email: &str) -> Result<(), VerificationError> {
        let email_normalized = email.to_lowercase();
        let user = match self.find_user_by_email(&email_normalized).await? {
            Some(u) => u,
            None => return Ok(()),
        };

        if user.email_verified {
            return Ok(());
        }

        let now = Utc::now();
        let mut active_model: ActiveModel = user.into();
        active_model.email_verified = Set(true);
        active_model.updated_at = Set(now);
        active_model
            .update(&self.db)
            .await
            .context("Failed to auto-verify email")?;

        tracing::info!(
            "Auto-verified email for {} (email service not configured)",
            email_normalized
        );
        Ok(())
    }

    /// Verify email with code validation and brute-force protection.
    ///
    /// The flow is:
    /// 1. Fetch user; for non-existent users, perform a dummy Argon2 verify
    ///    so the timing profile matches the existent-user path.
    /// 2. Atomically increment the failed-attempts counter (which also
    ///    enforces the `MAX_FAILED_ATTEMPTS` threshold in the same
    ///    `UPDATE … WHERE failed_attempts < MAX` statement). This eliminates
    ///    the previous TOCTOU race where N concurrent requests could all
    ///    pass the threshold check and all run Argon2 before any increment
    ///    landed.
    /// 3. Only if the increment succeeded (i.e. the user was below the
    ///    threshold) do we run the expensive Argon2 verification.
    /// 4. On a correct code, reset the counter to 0 and mark verified.
    pub async fn verify_email(&self, email: &str, code: &str) -> Result<(), VerificationError> {
        let email_normalized = email.to_lowercase();

        // Fetch user to get stored hash and expires_at.
        // When the user does not exist we perform a dummy Argon2
        // verification so that the non-existent-user path has the same
        // timing profile as the existent-user path.
        let user = match self.find_user_by_email(&email_normalized).await? {
            Some(u) => u,
            None => {
                let _ = Self::verify_code(code, dummy_code_hash());
                return Err(VerificationError::InvalidOrExpired);
            }
        };

        // Anti-enumeration: even if the user is already verified, perform a
        // dummy Argon2 verification and return the same error as non-existent
        // users. Returning a distinct "AlreadyVerified" error would allow an
        // attacker to distinguish "user exists and verified" from "user does
        // not exist" by comparing error responses.
        if user.email_verified {
            let _ = Self::verify_code(code, dummy_code_hash());
            return Err(VerificationError::InvalidOrExpired);
        }

        let code_hash = user
            .verification_code_hash
            .as_deref()
            .ok_or(VerificationError::InvalidOrExpired)?;

        let expires_at = user
            .verification_code_expires_at
            .ok_or(VerificationError::InvalidOrExpired)?;

        if Utc::now() > expires_at {
            return Err(VerificationError::InvalidOrExpired);
        }

        // Atomic claim: increment the counter only if the user is below
        // the threshold.  This is the only place the counter is mutated
        // by verify_email, and the threshold check is part of the same
        // SQL statement, so concurrent requests cannot all run Argon2.
        self.increment_failed_attempts(&user.id).await?;

        // Verify code against stored hash
        if !Self::verify_code(code, code_hash)? {
            return Err(VerificationError::InvalidOrExpired);
        }

        // Atomic verification: clear code, mark verified, and reset the
        // counter (which was just incremented by the claim above) in a
        // single ActiveModel update.
        let now = Utc::now();
        let mut active_model: ActiveModel = user.into();
        active_model.email_verified = Set(true);
        active_model.verification_code_hash = Set(None);
        active_model.verification_code_expires_at = Set(None);
        active_model.verification_code_sent_at = Set(None);
        active_model.verification_failed_attempts = Set(0);
        active_model.updated_at = Set(now);
        let updated_user = active_model
            .update(&self.db)
            .await
            .context("Failed to verify email")?;

        if self.email.is_configured().await {
            self.send_welcome_email_background(&email_normalized, &updated_user.name);
        }

        tracing::info!("Email verified for {}", email_normalized);
        Ok(())
    }

    /// Increment the failed-attempts counter atomically, enforcing the brute-force
    /// threshold.  Uses a single `UPDATE … WHERE failed_attempts < MAX` statement
    /// to eliminate the TOCTOU gap between the read check and the write increment.
    async fn increment_failed_attempts(
        &self,
        user_id: &uuid::Uuid,
    ) -> Result<(), VerificationError> {
        let result = UserEntity::update_many()
            .col_expr(
                Column::VerificationFailedAttempts,
                sea_orm::sea_query::Expr::col(Column::VerificationFailedAttempts).add(1),
            )
            .filter(Column::Id.eq(*user_id))
            .filter(Column::VerificationFailedAttempts.lt(MAX_FAILED_ATTEMPTS))
            .exec(&self.db)
            .await
            .context("Failed to increment failed attempts")?;

        // When the threshold was already hit concurrently, SeaORM still reports
        // rows_affected = 0 because the WHERE clause excluded the row.
        if result.rows_affected == 0 {
            return Err(VerificationError::TooManyAttempts);
        }
        Ok(())
    }

    pub async fn resend_code(&self, email: &str) -> Result<(), VerificationError> {
        let email_normalized = email.to_lowercase();

        // Anti-enumeration: look up the user BEFORE checking SMTP
        // configuration.  Non-existent emails always receive 200
        // regardless of SMTP state so that an attacker cannot infer
        // user existence from a 503 response.
        let user = match self.find_user_by_email(&email_normalized).await? {
            Some(u) => u,
            None => {
                // Anti-timing-attack: perform a dummy Argon2 hash to
                // match the timing profile of the existent-user path
                // (which always calls hash_code).
                let _ = Self::hash_code("000000");
                return Ok(());
            }
        };

        // Already-verified users don't need another code.
        // This check MUST come before the email-config check so that
        // auto-verified users (dev/test environments without SMTP) get
        // 200 OK rather than 503 EmailNotConfigured.
        if user.email_verified {
            return Ok(());
        }

        // Only check SMTP configuration after confirming the user exists
        // and is not already verified.
        if !self.email.is_configured().await {
            return Err(VerificationError::EmailNotConfigured);
        }

        let now = Utc::now();

        // Generate the code eagerly so that code-creation cost is always
        // paid before we open any DB transaction or lock.
        let code = Self::generate_code();
        let code_hash = Self::hash_code(&code)?;
        let expires_at = now + Duration::minutes(CODE_EXPIRY_MINUTES);

        // ── Atomic cooldown enforcement ──────────────────────────────────
        // The previous read → check → write pattern had a TOCTOU race:
        // two concurrent requests could both observe an old sent_at,
        // both pass the 60 s check, and both issue a new code.
        //
        // We now fold the cooldown predicate into a single UPDATE ... WHERE
        // statement.  SeaORM reports rows_affected = 0 when the WHERE
        // clause eliminates the row, which means either the row was
        // deleted or the cooldown predicate did not hold.
        let cooldown_threshold = now - Duration::seconds(RESEND_COOLDOWN_SECONDS);

        let result = UserEntity::update_many()
            .col_expr(Column::VerificationCodeHash, Expr::value(code_hash))
            .col_expr(Column::VerificationCodeExpiresAt, Expr::value(expires_at))
            .col_expr(Column::VerificationCodeSentAt, Expr::value(now))
            .col_expr(Column::VerificationFailedAttempts, Expr::value(0))
            .filter(Column::Id.eq(user.id))
            .filter(
                Column::VerificationCodeSentAt
                    .is_null()
                    .or(Column::VerificationCodeSentAt.lte(cooldown_threshold)),
            )
            .exec(&self.db)
            .await
            .context("Failed to store verification code")?;

        if result.rows_affected == 0 {
            return Err(VerificationError::TooSoon);
        }

        // Send synchronously so the caller can handle failures.
        let _permit = self.email_send_limiter.acquire().await.map_err(|e| {
            VerificationError::Internal(anyhow::anyhow!("Email send semaphore closed: {:?}", e))
        })?;
        self.email
            .send_registration_code_email(&email_normalized, &code, CODE_EXPIRY_MINUTES)
            .await
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send verification email to {}: {:?}",
                    email_normalized,
                    e
                );
            })
            .map_err(|e| {
                VerificationError::Internal(anyhow::anyhow!(
                    "Failed to send verification email: {}",
                    e
                ))
            })?;

        tracing::info!("Verification code resent to {}", email_normalized);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_is_six_digits() {
        for _ in 0..100 {
            let code = VerificationService::generate_code();
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
        let hash = VerificationService::hash_code(code).unwrap();
        assert!(VerificationService::verify_code(code, &hash).unwrap());
        assert!(!VerificationService::verify_code("654321", &hash).unwrap());
    }

    #[test]
    fn test_different_codes_produce_different_hashes() {
        let hash1 = VerificationService::hash_code("111111").unwrap();
        let hash2 = VerificationService::hash_code("222222").unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_same_code_produce_different_hashes_due_to_salt() {
        let hash1 = VerificationService::hash_code("123456").unwrap();
        let hash2 = VerificationService::hash_code("123456").unwrap();
        // Argon2 uses random salt, so hashes differ even for same input
        assert_ne!(hash1, hash2);
        // But both verify correctly
        assert!(VerificationService::verify_code("123456", &hash1).unwrap());
        assert!(VerificationService::verify_code("123456", &hash2).unwrap());
    }
}
