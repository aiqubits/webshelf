//! Pluggable storage abstractions.
//!
//! The SDK deliberately does **not** depend on any specific cache or database
//! backend. Instead it defines three small async traits that the host
//! application implements:
//!
//! | Trait              | Purpose                                                   |
//! |--------------------|-----------------------------------------------------------|
//! | [`CaptchaStore`]   | Store / fetch / consume captcha codes with TTL            |
//! | [`UserBindingStore`]| Look up a user id by openid and bind openid ↔ user id    |
//! | [`AccessTokenStore`]| Cache WeChat `access_token` between API calls            |
//!
//! This keeps the SDK reusable across Redis, in-memory, or database-backed
//! deployments and lets multiple WeChat accounts share a single store by
//! namespacing keys with `account_id`.

use async_trait::async_trait;
use std::time::Duration;

use crate::error::WechatResult;

/// Key-value store for captcha codes, with TTL and one-time-consume semantics.
///
/// The host implementation is responsible for namespacing keys per account
/// (e.g. `wechat:{account_id}:captcha:{code}`) so multiple official accounts
/// never collide.
#[async_trait]
pub trait CaptchaStore: Send + Sync {
    /// Store `code` under `key` with the given `ttl`.
    /// Overwrites any existing value.
    async fn set_captcha(
        &self,
        key: &str,
        code: &str,
        openid: &str,
        ttl: Duration,
    ) -> WechatResult<()>;

    /// Atomically store `code` under `key` **only if the key does not already
    /// exist**, with the given `ttl`.
    ///
    /// Returns `true` if the captcha was stored, `false` if a captcha already
    /// exists under `key` (cooldown). This is the atomic equivalent of
    /// `peek_captcha` + `set_captcha` but without the TOCTOU race.
    async fn set_captcha_nx(
        &self,
        key: &str,
        code: &str,
        openid: &str,
        ttl: Duration,
    ) -> WechatResult<bool>;

    /// Atomically read-and-delete the captcha under `key`.
    ///
    /// Returns `Ok(Some((code, openid)))` on hit, `Ok(None)` when the key
    /// does not exist or has expired. The one-shot semantics prevent a
    /// single captcha from being verified more than once.
    async fn consume_captcha(&self, key: &str) -> WechatResult<Option<(String, String)>>;

    /// Peek at the captcha without deleting it (used for cooldown checks).
    async fn peek_captcha(&self, key: &str) -> WechatResult<Option<(String, String)>>;

    /// Increment and return the failed-attempt counter for `key`.
    /// Resets to 0 when a new captcha is stored via [`set_captcha`].
    async fn incr_failed_attempts(&self, key: &str) -> WechatResult<u32>;

    /// Reset the failed-attempt counter to 0 for `key`.
    async fn reset_failed_attempts(&self, key: &str) -> WechatResult<()>;

    /// Delete the captcha and its associated counter for `key`.
    async fn delete_captcha(&self, key: &str) -> WechatResult<()>;
}

/// Store mapping WeChat `openid` ↔ application user id.
///
/// Implementations typically back this with a database table
/// (e.g. `users.wx_openid`).
#[async_trait]
pub trait UserBindingStore: Send + Sync {
    /// The user-id type returned to the host application. The SDK is generic
    /// over this so it stays decoupled from any particular user model.
    type UserId: Send + Sync + Clone;

    /// Look up the user id bound to `openid`.
    /// Returns `Ok(None)` when no user is bound.
    async fn find_user_by_openid(&self, openid: &str) -> WechatResult<Option<Self::UserId>>;

    /// Bind (or rebind) `openid` to `user_id`.
    async fn bind_openid(&self, user_id: &Self::UserId, openid: &str) -> WechatResult<()>;

    /// Unbind any openid from `user_id`.
    async fn unbind_openid(&self, user_id: &Self::UserId) -> WechatResult<()>;
}

/// Cache for WeChat Platform `access_token` values.
///
/// Access tokens are valid for ~2 hours and have strict rate limits on
/// retrieval, so they should be cached and shared across all API calls for
/// a given AppID.
#[cfg(feature = "client")]
#[async_trait]
pub trait AccessTokenStore: Send + Sync {
    /// Retrieve a cached access token for `app_id`.
    async fn get_access_token(&self, app_id: &str) -> WechatResult<Option<String>>;

    /// Store an access token for `app_id` with `ttl`.
    async fn set_access_token(&self, app_id: &str, token: &str, ttl: Duration) -> WechatResult<()>;
}

// ── In-memory implementations (behind the `memory-store` feature) ───────────

#[cfg(feature = "memory-store")]
pub mod memory {
    //! Simple in-memory store implementations backed by `tokio::sync` maps.
    //! Useful for tests, single-instance dev deployments, and examples.

    use super::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use tokio::sync::Mutex;

    type CaptchaEntry = (String, String, Option<Instant>); // (code, openid, expiry)
    type AttemptEntry = u32;

    /// In-memory [`CaptchaStore`] for tests / single-instance use.
    #[derive(Debug, Default)]
    pub struct MemoryCaptchaStore {
        captchas: Mutex<HashMap<String, CaptchaEntry>>,
        attempts: Mutex<HashMap<String, AttemptEntry>>,
    }

    impl MemoryCaptchaStore {
        pub fn new() -> Self {
            Self::default()
        }
    }

    #[async_trait]
    impl CaptchaStore for MemoryCaptchaStore {
        async fn set_captcha(
            &self,
            key: &str,
            code: &str,
            openid: &str,
            ttl: Duration,
        ) -> WechatResult<()> {
            let expiry = Instant::now().checked_add(ttl);
            self.captchas.lock().await.insert(
                key.to_string(),
                (code.to_string(), openid.to_string(), expiry),
            );
            self.attempts.lock().await.insert(key.to_string(), 0);
            Ok(())
        }

        async fn set_captcha_nx(
            &self,
            key: &str,
            code: &str,
            openid: &str,
            ttl: Duration,
        ) -> WechatResult<bool> {
            let mut captchas = self.captchas.lock().await;
            if captchas.contains_key(key) {
                return Ok(false);
            }
            let expiry = Instant::now().checked_add(ttl);
            captchas.insert(
                key.to_string(),
                (code.to_string(), openid.to_string(), expiry),
            );
            self.attempts.lock().await.insert(key.to_string(), 0);
            Ok(true)
        }

        async fn consume_captcha(&self, key: &str) -> WechatResult<Option<(String, String)>> {
            let mut map = self.captchas.lock().await;
            if let Some((code, openid, expiry)) = map.remove(key) {
                if let Some(exp) = expiry
                    && Instant::now() > exp
                {
                    self.attempts.lock().await.remove(key);
                    return Ok(None);
                }
                Ok(Some((code, openid)))
            } else {
                Ok(None)
            }
        }

        async fn peek_captcha(&self, key: &str) -> WechatResult<Option<(String, String)>> {
            let map = self.captchas.lock().await;
            if let Some((code, openid, expiry)) = map.get(key) {
                if let Some(exp) = expiry
                    && Instant::now() > *exp
                {
                    return Ok(None);
                }
                Ok(Some((code.clone(), openid.clone())))
            } else {
                Ok(None)
            }
        }

        async fn incr_failed_attempts(&self, key: &str) -> WechatResult<u32> {
            let mut map = self.attempts.lock().await;
            let count = map.entry(key.to_string()).or_insert(0);
            *count += 1;
            Ok(*count)
        }

        async fn reset_failed_attempts(&self, key: &str) -> WechatResult<()> {
            self.attempts.lock().await.insert(key.to_string(), 0);
            Ok(())
        }

        async fn delete_captcha(&self, key: &str) -> WechatResult<()> {
            self.captchas.lock().await.remove(key);
            self.attempts.lock().await.remove(key);
            Ok(())
        }
    }

    /// In-memory [`UserBindingStore`] keyed by `String` user ids.
    #[derive(Debug, Default)]
    pub struct MemoryUserBindingStore {
        openid_to_user: Mutex<HashMap<String, String>>,
        user_to_openid: Mutex<HashMap<String, String>>,
    }

    impl MemoryUserBindingStore {
        pub fn new() -> Self {
            Self::default()
        }
    }

    #[async_trait]
    impl UserBindingStore for MemoryUserBindingStore {
        type UserId = String;

        async fn find_user_by_openid(&self, openid: &str) -> WechatResult<Option<Self::UserId>> {
            Ok(self.openid_to_user.lock().await.get(openid).cloned())
        }

        async fn bind_openid(&self, user_id: &Self::UserId, openid: &str) -> WechatResult<()> {
            self.openid_to_user
                .lock()
                .await
                .insert(openid.to_string(), user_id.clone());
            self.user_to_openid
                .lock()
                .await
                .insert(user_id.clone(), openid.to_string());
            Ok(())
        }

        async fn unbind_openid(&self, user_id: &Self::UserId) -> WechatResult<()> {
            if let Some(openid) = self.user_to_openid.lock().await.remove(user_id) {
                self.openid_to_user.lock().await.remove(&openid);
            }
            Ok(())
        }
    }

    /// In-memory [`AccessTokenStore`] for tests / single-instance use.
    #[cfg(feature = "client")]
    #[derive(Debug, Default)]
    pub struct MemoryAccessTokenStore {
        tokens: Mutex<HashMap<String, (String, Instant)>>,
    }

    #[cfg(feature = "client")]
    impl MemoryAccessTokenStore {
        pub fn new() -> Self {
            Self::default()
        }
    }

    #[cfg(feature = "client")]
    #[async_trait]
    impl AccessTokenStore for MemoryAccessTokenStore {
        async fn get_access_token(&self, app_id: &str) -> WechatResult<Option<String>> {
            let map = self.tokens.lock().await;
            if let Some((token, expiry)) = map.get(app_id)
                && Instant::now() < *expiry
            {
                return Ok(Some(token.clone()));
            }
            Ok(None)
        }

        async fn set_access_token(
            &self,
            app_id: &str,
            token: &str,
            ttl: Duration,
        ) -> WechatResult<()> {
            let expiry = Instant::now() + ttl;
            self.tokens
                .lock()
                .await
                .insert(app_id.to_string(), (token.to_string(), expiry));
            Ok(())
        }
    }
}

#[cfg(test)]
#[cfg(feature = "memory-store")]
mod tests {
    use super::memory::*;
    use std::time::Duration;

    // Bring the traits into scope so trait methods resolve on the memory
    // store structs.
    use crate::store::{CaptchaStore, UserBindingStore};

    #[tokio::test]
    async fn test_captcha_lifecycle() {
        let store = MemoryCaptchaStore::new();
        store
            .set_captcha("k1", "AB123", "openid_x", Duration::from_secs(60))
            .await
            .unwrap();

        // peek
        let peeked = store.peek_captcha("k1").await.unwrap();
        assert_eq!(peeked.unwrap().0, "AB123");

        // consume (one-shot)
        let consumed = store.consume_captcha("k1").await.unwrap();
        assert_eq!(consumed.unwrap().0, "AB123");

        // second consume returns None
        assert!(store.consume_captcha("k1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_captcha_attempts() {
        let store = MemoryCaptchaStore::new();
        assert_eq!(store.incr_failed_attempts("k2").await.unwrap(), 1);
        assert_eq!(store.incr_failed_attempts("k2").await.unwrap(), 2);
        store.reset_failed_attempts("k2").await.unwrap();
        assert_eq!(store.incr_failed_attempts("k2").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_user_binding() {
        let store = MemoryUserBindingStore::new();
        store
            .bind_openid(&"user_1".to_string(), "openid_y")
            .await
            .unwrap();
        assert_eq!(
            store.find_user_by_openid("openid_y").await.unwrap(),
            Some("user_1".to_string())
        );
        store.unbind_openid(&"user_1".to_string()).await.unwrap();
        assert!(
            store
                .find_user_by_openid("openid_y")
                .await
                .unwrap()
                .is_none()
        );
    }
}
