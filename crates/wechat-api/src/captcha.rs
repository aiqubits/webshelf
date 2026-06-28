//! Captcha generation, storage, and one-shot verification.
//!
//! This module implements the core of the WeChat captcha-login flow:
//!
//! 1. [`CaptchaService::generate`] — creates a random alphanumeric code,
//!    stores it under a namespaced key bound to the caller's `openid`,
//!    and returns the code so it can be pushed to the user (typically via
//!    a WeChat customer-service message).
//! 2. [`CaptchaService::verify`] — atomically consumes the stored code and
//!    checks it against the user-supplied value. A failed attempt
//!    increments a counter; after `max_failed_attempts` the code is
//!    invalidated.

use std::sync::Arc;
use std::time::Duration;

use rand::Rng;

use crate::config::LoginConfig;
use crate::error::{WechatError, WechatResult};
use crate::store::CaptchaStore;

/// Character set for captcha codes. Excludes ambiguous characters
/// (0/O, 1/I/l) to reduce user-input errors over a WeChat text channel.
const CHARSET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// Service for generating and verifying WeChat login captchas.
pub struct CaptchaService {
    store: Arc<dyn CaptchaStore>,
    config: LoginConfig,
}

impl CaptchaService {
    /// Create a new captcha service backed by the given store.
    pub fn new(store: Arc<dyn CaptchaStore>, config: LoginConfig) -> Self {
        Self { store, config }
    }

    /// Returns the captcha TTL in seconds.
    pub fn captcha_ttl(&self) -> u64 {
        self.config.captcha_ttl_secs
    }

    /// Generate a random captcha code of the configured length.
    pub fn generate_code(&self) -> String {
        let mut rng = rand::thread_rng();
        (0..self.config.captcha_len)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Build the namespaced cache key for a captcha.
    ///
    /// The key incorporates `account_id` and `openid` so that:
    /// - Multiple official accounts don't collide.
    /// - Each user has at most one active captcha at a time.
    pub fn captcha_key(&self, account_id: &str, openid: &str) -> String {
        format!("wechat:{account_id}:captcha:{openid}")
    }

    /// Generate, store, and return a new captcha for the given user.
    ///
    /// The caller is responsible for actually delivering the returned code
    /// to the user (e.g. via [`crate::client::WechatClient::send_text_message`]).
    ///
    /// # Cooldown
    ///
    /// If a captcha already exists for this user (i.e. it was generated
    /// recently and hasn't been consumed or expired), this returns
    /// [`WechatError::CooldownActive`]. The caller can choose to silently
    /// ignore this in the message-handler path.
    pub async fn generate(&self, account_id: &str, openid: &str) -> WechatResult<String> {
        let key = self.captcha_key(account_id, openid);
        let code = self.generate_code();
        let ttl = Duration::from_secs(self.config.captcha_ttl_secs);

        // Atomic check-and-set via set_captcha_nx: only succeeds when no
        // unconsumed captcha exists for this user.  Unlike the previous
        // peek_captcha + set_captcha pattern, this has no TOCTOU race.
        if !self.store.set_captcha_nx(&key, &code, openid, ttl).await? {
            return Err(WechatError::CooldownActive);
        }

        tracing::info!(account_id = %account_id, "Generated WeChat login captcha");
        Ok(code)
    }

    /// Verify a captcha for a known openid (the standard web-login path).
    ///
    /// Returns `Ok(openid)` on success (echoing back the verified openid for
    /// convenience in the login orchestrator).
    pub async fn verify_for_openid(
        &self,
        account_id: &str,
        openid: &str,
        code: &str,
    ) -> WechatResult<String> {
        let key = self.captcha_key(account_id, openid);

        // Peek (don't consume yet) so a wrong code doesn't burn the captcha.
        let stored = match self.store.peek_captcha(&key).await? {
            Some(v) => v,
            None => return Err(WechatError::CaptchaNotFound),
        };

        if !stored.0.eq_ignore_ascii_case(code) {
            let attempts = self.store.incr_failed_attempts(&key).await?;
            if attempts >= self.config.max_failed_attempts {
                self.store.delete_captcha(&key).await?;
                tracing::warn!(
                    account_id = %account_id,
                    attempts,
                    "Captcha invalidated after too many failed attempts"
                );
                return Err(WechatError::TooManyAttempts);
            }
            return Err(WechatError::CaptchaMismatch);
        }

        // Correct code — consume (one-shot) and reset attempts.
        // IMPORTANT: consume_captcha must return Some to confirm the captcha
        // was atomically claimed. If it returns None (race: another request
        // already consumed it), treat it as CaptchaNotFound rather than
        // silently succeeding.
        if self.store.consume_captcha(&key).await?.is_none() {
            tracing::warn!(
                account_id = %account_id,
                "Captcha already consumed by another request (race detected)"
            );
            return Err(WechatError::CaptchaNotFound);
        }
        self.store.reset_failed_attempts(&key).await?;
        tracing::info!(account_id = %account_id, "WeChat captcha verified successfully");
        Ok(openid.to_string())
    }

    /// Check whether `content` matches any configured trigger keyword.
    ///
    /// Used by the message handler to decide whether an incoming text
    /// message should trigger captcha generation.
    pub fn matches_trigger(&self, content: &str) -> bool {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.config.trigger_keywords.iter().any(|kw| {
            // Exact case-insensitive match always wins.
            if kw.eq_ignore_ascii_case(trimmed) {
                return true;
            }
            // For purely ASCII keywords (e.g. "login"), require word
            // boundaries to avoid false positives (e.g. "belonging").
            // Chinese / non-ASCII keywords fall through to the broader
            // contains check.
            if kw.is_ascii() {
                let lower = trimmed.to_ascii_lowercase();
                let kw_lower = kw.to_ascii_lowercase();
                lower
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|w| w == kw_lower)
            } else {
                trimmed.contains(kw.as_str())
            }
        })
    }
}

#[cfg(test)]
#[cfg(feature = "memory-store")]
mod tests {
    use super::*;
    use crate::store::memory::MemoryCaptchaStore;

    fn test_config() -> LoginConfig {
        LoginConfig {
            captcha_len: 5,
            max_failed_attempts: 3,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_generate_and_verify_success() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let svc = CaptchaService::new(store, test_config());

        let code = svc.generate("acct", "oUser").await.unwrap();
        let verified = svc.verify_for_openid("acct", "oUser", &code).await.unwrap();
        assert_eq!(verified, "oUser");
    }

    #[tokio::test]
    async fn test_verify_wrong_code_increments_attempts() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let svc = CaptchaService::new(store, test_config());

        let code = svc.generate("acct", "oUser").await.unwrap();

        // Two wrong attempts (max is 3).
        assert!(
            svc.verify_for_openid("acct", "oUser", "WRONG")
                .await
                .is_err()
        );
        assert!(
            svc.verify_for_openid("acct", "oUser", "WRONG")
                .await
                .is_err()
        );

        // Third wrong attempt invalidates.
        let err = svc
            .verify_for_openid("acct", "oUser", "WRONG")
            .await
            .unwrap_err();
        assert!(matches!(err, WechatError::TooManyAttempts));

        // The correct code no longer works (captcha was deleted).
        let err = svc
            .verify_for_openid("acct", "oUser", &code)
            .await
            .unwrap_err();
        assert!(matches!(err, WechatError::CaptchaNotFound));
    }

    #[tokio::test]
    async fn test_verify_case_insensitive() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let svc = CaptchaService::new(store, test_config());

        let code = svc.generate("acct", "oUser").await.unwrap();
        let lower = code.to_lowercase();
        assert!(svc.verify_for_openid("acct", "oUser", &lower).await.is_ok());
    }

    #[tokio::test]
    async fn test_cooldown_blocks_duplicate_generate() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let svc = CaptchaService::new(store, test_config());

        svc.generate("acct", "oUser").await.unwrap();
        let err = svc.generate("acct", "oUser").await.unwrap_err();
        assert!(matches!(err, WechatError::CooldownActive));
    }

    #[tokio::test]
    async fn test_generate_after_consume_allows_new() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let svc = CaptchaService::new(store, test_config());

        let code = svc.generate("acct", "oUser").await.unwrap();
        svc.verify_for_openid("acct", "oUser", &code).await.unwrap();
        // After consume, a new captcha can be generated.
        assert!(svc.generate("acct", "oUser").await.is_ok());
    }

    #[test]
    fn test_generate_code_length_and_charset() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let svc = CaptchaService::new(
            store,
            LoginConfig {
                captcha_len: 6,
                ..Default::default()
            },
        );
        for _ in 0..50 {
            let code = svc.generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| CHARSET.contains(&(c as u8))));
        }
    }

    #[test]
    fn test_matches_trigger() {
        let svc = CaptchaService::new(Arc::new(MemoryCaptchaStore::new()), LoginConfig::default());
        // Chinese keywords work via contains.
        assert!(svc.matches_trigger("验证码"));
        assert!(svc.matches_trigger("请发登录码"));
        // Exact ASCII match.
        assert!(svc.matches_trigger("login"));
        assert!(svc.matches_trigger("LOGIN"));
        // Word-boundary match for ASCII keywords.
        assert!(svc.matches_trigger("login now"));
        assert!(svc.matches_trigger("please login"));
        // No false positive on word-internal match.
        assert!(!svc.matches_trigger("belonging"));
        assert!(!svc.matches_trigger("login123"));
        // Empty / non-matching.
        assert!(!svc.matches_trigger(""));
        assert!(!svc.matches_trigger("hello"));
    }

    #[test]
    fn test_captcha_key_namespacing() {
        let svc = CaptchaService::new(Arc::new(MemoryCaptchaStore::new()), LoginConfig::default());
        let k1 = svc.captcha_key("acct1", "oUser");
        let k2 = svc.captcha_key("acct2", "oUser");
        let k3 = svc.captcha_key("acct1", "oOther");
        assert_ne!(k1, k2);
        assert_ne!(k1, k3);
        assert!(k1.starts_with("wechat:acct1:captcha:"));
    }
}
