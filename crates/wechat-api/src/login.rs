//! High-level orchestration of the WeChat captcha login flow.
//!
//! This module ties together [`CaptchaService`] (code generation /
//! verification) and [`UserBindingStore`] (openid → user id lookup) to
//! provide a single entry point for the web login endpoint:
//!
//! ```text
//! POST /auth/wx-login  { openid, code }
//!     ↓
//! LoginService::verify_and_login(openid, code)
//!     ↓ verify captcha
//!     ↓ look up user by openid
//!     → user_id  (host app then issues its own JWT / session)
//! ```
//!
//! The SDK intentionally does **not** issue JWTs — that is the host
//! application's responsibility (webshelf has its own auth layer). The
//! `LoginService` returns the verified `UserId`, leaving token issuance
//! to the caller.

use std::sync::Arc;

use crate::captcha::CaptchaService;
use crate::error::{WechatError, WechatResult};
use crate::store::UserBindingStore;

/// High-level service orchestrating the WeChat captcha login flow.
pub struct LoginService<U>
where
    U: UserBindingStore,
{
    pub captcha: Arc<CaptchaService>,
    pub bindings: Arc<U>,
}

/// The successful result of a captcha login: the verified openid and the
/// user id it is bound to.
#[derive(Debug, Clone)]
pub struct VerifiedLogin<U: UserBindingStore> {
    pub openid: String,
    pub user_id: U::UserId,
}

impl<U> LoginService<U>
where
    U: UserBindingStore,
{
    /// Create a new login service.
    pub fn new(captcha: Arc<CaptchaService>, bindings: Arc<U>) -> Self {
        Self { captcha, bindings }
    }

    /// Verify the captcha for `openid` and return the bound user id.
    ///
    /// # Flow
    ///
    /// 1. Verify the captcha code against the stored value (one-shot
    ///    consume, brute-force protected).
    /// 2. Look up the user bound to `openid`.
    /// 3. Return the [`VerifiedLogin`] for the host to issue a session.
    ///
    /// # Errors
    ///
    /// - [`WechatError::CaptchaNotFound`] / [`CaptchaMismatch`] /
    ///   [`TooManyAttempts`] — captcha validation failed.
    /// - [`WechatError::UserNotBound`] — no user is bound to the openid.
    pub async fn verify_and_login(
        &self,
        account_id: &str,
        openid: &str,
        code: &str,
    ) -> WechatResult<VerifiedLogin<U>> {
        // 1. Verify captcha (consumes it on success).
        let verified_openid = self
            .captcha
            .verify_for_openid(account_id, openid, code)
            .await?;

        // 2. Look up bound user.
        let user_id = self
            .bindings
            .find_user_by_openid(&verified_openid)
            .await?
            .ok_or_else(|| WechatError::UserNotBound(verified_openid.clone()))?;

        tracing::info!(
            openid = %verified_openid,
            "WeChat captcha login verified"
        );

        Ok(VerifiedLogin {
            openid: verified_openid,
            user_id,
        })
    }
}

#[cfg(test)]
#[cfg(feature = "memory-store")]
mod tests {
    use super::*;
    use crate::config::LoginConfig;
    use crate::store::memory::{MemoryCaptchaStore, MemoryUserBindingStore};

    #[tokio::test]
    async fn test_verify_and_login_success() {
        let captcha_store = Arc::new(MemoryCaptchaStore::new());
        let captcha = Arc::new(CaptchaService::new(captcha_store, LoginConfig::default()));
        let bindings = Arc::new(MemoryUserBindingStore::new());

        // Pre-bind a user.
        bindings
            .bind_openid(&"user_42".to_string(), "oUser")
            .await
            .unwrap();

        // Generate a captcha (simulating the WeChat message-trigger path).
        let code = captcha.generate("acct", "oUser").await.unwrap();

        let svc = LoginService::new(captcha, bindings);
        let result = svc.verify_and_login("acct", "oUser", &code).await.unwrap();
        assert_eq!(result.openid, "oUser");
        assert_eq!(result.user_id, "user_42");
    }

    #[tokio::test]
    async fn test_verify_and_login_wrong_code() {
        let captcha_store = Arc::new(MemoryCaptchaStore::new());
        let captcha = Arc::new(CaptchaService::new(captcha_store, LoginConfig::default()));
        let bindings = Arc::new(MemoryUserBindingStore::new());
        bindings
            .bind_openid(&"user_42".to_string(), "oUser")
            .await
            .unwrap();

        captcha.generate("acct", "oUser").await.unwrap();

        let svc = LoginService::new(captcha, bindings);
        let err = svc
            .verify_and_login("acct", "oUser", "WRONG")
            .await
            .unwrap_err();
        assert!(matches!(err, WechatError::CaptchaMismatch));
    }

    #[tokio::test]
    async fn test_verify_and_login_user_not_bound() {
        let captcha_store = Arc::new(MemoryCaptchaStore::new());
        let captcha = Arc::new(CaptchaService::new(captcha_store, LoginConfig::default()));
        let bindings = Arc::new(MemoryUserBindingStore::new());

        let code = captcha.generate("acct", "oUser").await.unwrap();

        let svc = LoginService::new(captcha, bindings);
        let err = svc
            .verify_and_login("acct", "oUser", &code)
            .await
            .unwrap_err();
        assert!(matches!(err, WechatError::UserNotBound(_)));
    }

    #[tokio::test]
    async fn test_verify_and_login_captcha_consumed_after_success() {
        let captcha_store = Arc::new(MemoryCaptchaStore::new());
        let captcha = Arc::new(CaptchaService::new(captcha_store, LoginConfig::default()));
        let bindings = Arc::new(MemoryUserBindingStore::new());
        bindings
            .bind_openid(&"u".to_string(), "oUser")
            .await
            .unwrap();

        let code = captcha.generate("acct", "oUser").await.unwrap();
        let svc = LoginService::new(captcha.clone(), bindings);

        // First verification succeeds.
        svc.verify_and_login("acct", "oUser", &code).await.unwrap();

        // Second attempt with the same code fails (one-shot).
        let err = svc
            .verify_and_login("acct", "oUser", &code)
            .await
            .unwrap_err();
        assert!(matches!(err, WechatError::CaptchaNotFound));
    }
}
