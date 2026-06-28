//! WeChat Official Account integration service.
//!
//! Provides store trait implementations for the `wechat-api` crate and a
//! `WechatComponents` holder that bundles all WeChat services together.
//!
//! When `config.wechat.enabled` is `false` (the default), `init_wechat_components`
//! returns `None` and no WeChat routes are registered.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use sea_orm::{ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, QueryFilter, Statement};
use wechat_api::{
    CaptchaService, LoginConfig, LoginService, WechatConfig,
    store::{CaptchaStore, UserBindingStore},
};

use crate::services::CacheService;
use crate::utils::config::WechatAccountConfig;
use crate::utils::db_router::AutoRouter;

// ── Redis key helpers ──────────────────────────────────────────────────────

/// Build the reverse-index key `wechat:code:{code}` → openid.
fn code_index_key(account_id: &str, code: &str) -> String {
    format!("wechat:{account_id}:code:{code}")
}

// ── CaptchaStore implementation (Redis-backed) ─────────────────────────────

/// Redis-backed [`CaptchaStore`] delegating to [`CacheService`]'s Redis client.
pub struct RedisCaptchaStore {
    client: Option<redis::Client>,
    conn: tokio::sync::Mutex<Option<ConnectionManager>>,
    account_id: String,
}

impl RedisCaptchaStore {
    /// Create a new store.  When `redis_client` is `None` all operations
    /// silently no‑op (graceful degradation).
    pub fn new(account_id: &str, redis_client: Option<&redis::Client>) -> Self {
        Self {
            client: redis_client.cloned(),
            conn: tokio::sync::Mutex::new(None),
            account_id: account_id.to_string(),
        }
    }

    /// Lazily initialise the connection manager.
    ///
    /// Uses double-checked locking: the fast path (already initialised)
    /// releases the mutex immediately; the slow path releases it before
    /// the async network call to avoid blocking all Redis operations.
    async fn conn(&self) -> Option<ConnectionManager> {
        // Fast path: already initialised — lock is held only for Clone.
        {
            let guard = self.conn.lock().await;
            if let Some(ref cm) = *guard {
                return Some(cm.clone());
            }
        }
        // Slow path: initialise (mutex is released during async init).
        let cm = self.client.as_ref()?.get_connection_manager().await.ok()?;
        // Re-acquire and check if another task beat us to it.
        let mut guard = self.conn.lock().await;
        if guard.is_some() {
            return guard.clone();
        }
        *guard = Some(cm.clone());
        Some(cm)
    }

    async fn set_ex(&self, key: &str, value: &str, ttl_secs: u64) -> wechat_api::WechatResult<()> {
        let mut conn = self.conn().await.ok_or_else(|| {
            wechat_api::WechatError::Internal(anyhow::anyhow!("Redis not available"))
        })?;
        redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_secs)
            .arg(value)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!("Redis SETEX failed: {e}"))
            })
    }

    /// Atomically set `key` to `value` with TTL only if `key` does not
    /// already exist (SET NX + EX). Returns `true` if the key was set,
    /// `false` if it already existed.
    async fn set_nx_ex(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> wechat_api::WechatResult<bool> {
        let mut conn = self.conn().await.ok_or_else(|| {
            wechat_api::WechatError::Internal(anyhow::anyhow!("Redis not available"))
        })?;
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!("Redis SET NX+EX failed: {e}"))
            })?;
        Ok(result.is_some())
    }

    /// Look up a value by key from Redis (used by handler for code→openid reverse index).
    pub async fn get_opt(&self, key: &str) -> wechat_api::WechatResult<Option<String>> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(None),
        };
        redis::cmd("GET")
            .arg(key)
            .query_async::<Option<String>>(&mut conn)
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!("Redis GET failed: {e}"))
            })
    }

    async fn del(&self, key: &str) -> wechat_api::WechatResult<()> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(()),
        };
        redis::cmd("DEL")
            .arg(key)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!("Redis DEL failed: {e}"))
            })
    }

    async fn incr(&self, key: &str) -> wechat_api::WechatResult<u32> {
        let mut conn = self.conn().await.ok_or_else(|| {
            wechat_api::WechatError::Internal(anyhow::anyhow!("Redis not available"))
        })?;
        let val: u32 = redis::cmd("INCR")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!("Redis INCR failed: {e}"))
            })?;
        // Refresh TTL on every increment so the :attempts key self-cleans
        // shortly after the last attempt, even if the parent captcha key
        // expires naturally via SETEX.
        let _: () = redis::cmd("EXPIRE")
            .arg(key)
            .arg(600u64) // 10 minutes — shorter than captcha TTL + grace
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!("Redis EXPIRE failed: {e}"))
            })?;
        Ok(val)
    }
}

// Lua script for atomic captcha consumption.
//
// The script atomically:
// 1. GETs the captcha value ("{code}:{openid}") from KEYS[1]
// 2. DELETEs KEYS[1] and its failed-attempts counter
// 3. DELETEs the reverse-index key ("wechat:{id}:code:{code}") by deriving it
//    from the captcha key and the stored code value
// 4. Returns the stored value (nil if already consumed)
//
// Running all four operations in a single Lua script eliminates the TOCTOU
// race that would exist with separate GET+DEL commands.
const CONSUME_CAPTCHA_SCRIPT: &str = r#"
    local val = redis.call('GET', KEYS[1])
    if val then
        redis.call('DEL', KEYS[1])
        redis.call('DEL', KEYS[1] .. ':attempts')
        -- Extract code (part before first ':')
        local colon_pos = string.find(val, ':')
        if colon_pos then
            local code = string.sub(val, 1, colon_pos - 1)
            -- Derive the reverse-index key prefix from the captcha key
            -- KEYS[1] = 'wechat:{account_id}:captcha:{openid}'
            -- Need:   'wechat:{account_id}:code:{code}'
            local a_pos = string.find(KEYS[1], ':captcha:')
            if a_pos then
                local prefix = string.sub(KEYS[1], 1, a_pos - 1)
                redis.call('DEL', prefix .. ':code:' .. code)
            end
        end
    end
    return val
"#;

#[async_trait]
impl CaptchaStore for RedisCaptchaStore {
    async fn set_captcha(
        &self,
        key: &str,
        code: &str,
        openid: &str,
        ttl: Duration,
    ) -> wechat_api::WechatResult<()> {
        let ttl_secs = ttl.as_secs();
        // Store the captcha value as "code:openid"
        let value = format!("{code}:{openid}");
        self.set_ex(key, &value, ttl_secs).await?;

        // Also store the reverse index: code → openid (same TTL).
        let code_key = code_index_key(&self.account_id, code);
        self.set_ex(&code_key, openid, ttl_secs).await?;

        // Reset the failed-attempts counter (trait contract: "Resets to 0
        // when a new captcha is stored via set_captcha").
        self.del(&format!("{key}:attempts")).await?;
        Ok(())
    }

    async fn set_captcha_nx(
        &self,
        key: &str,
        code: &str,
        openid: &str,
        ttl: Duration,
    ) -> wechat_api::WechatResult<bool> {
        let ttl_secs = ttl.as_secs();
        let value = format!("{code}:{openid}");

        // Atomic check-and-set via SET NX + EX.
        if !self.set_nx_ex(key, &value, ttl_secs).await? {
            return Ok(false);
        }

        // Main key was atomically set — now store the reverse index
        // (best-effort, same TTL) and reset the failed-attempts counter.
        let code_key = code_index_key(&self.account_id, code);
        if let Err(e) = self.set_ex(&code_key, openid, ttl_secs).await {
            tracing::warn!(
                account_id = %self.account_id,
                error = %e,
                "Failed to store reverse index for captcha — code→openid lookup may fail"
            );
        }
        self.del(&format!("{key}:attempts")).await?;
        Ok(true)
    }

    async fn consume_captcha(
        &self,
        key: &str,
    ) -> wechat_api::WechatResult<Option<(String, String)>> {
        let mut conn = match self.conn().await {
            Some(c) => c,
            None => return Ok(None),
        };

        // Atomically GET + DEL (main key, attempts, reverse index).
        let val: Option<String> = redis::cmd("EVAL")
            .arg(CONSUME_CAPTCHA_SCRIPT)
            .arg(1_usize)
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!(
                    "Redis atomic consume failed: {e}"
                ))
            })?;

        match val {
            Some(stored) => {
                // Parse "code:openid"
                let parts: Vec<&str> = stored.splitn(2, ':').collect();
                if parts.len() < 2 {
                    return Ok(None);
                }
                Ok(Some((parts[0].to_string(), parts[1].to_string())))
            }
            None => Ok(None),
        }
    }

    async fn peek_captcha(&self, key: &str) -> wechat_api::WechatResult<Option<(String, String)>> {
        let stored = self.get_opt(key).await?;
        match stored {
            Some(val) => {
                let parts: Vec<&str> = val.splitn(2, ':').collect();
                if parts.len() < 2 {
                    return Ok(None);
                }
                let code = parts[0].to_string();
                let openid = parts[1].to_string();
                Ok(Some((code, openid)))
            }
            None => Ok(None),
        }
    }

    async fn incr_failed_attempts(&self, key: &str) -> wechat_api::WechatResult<u32> {
        let attempt_key = format!("{key}:attempts");
        self.incr(&attempt_key).await
    }

    async fn reset_failed_attempts(&self, key: &str) -> wechat_api::WechatResult<()> {
        let attempt_key = format!("{key}:attempts");
        self.del(&attempt_key).await
    }

    async fn delete_captcha(&self, key: &str) -> wechat_api::WechatResult<()> {
        // Try to peek to get the code for removing the reverse index.
        if let Some((code, _)) = self.peek_captcha(key).await? {
            self.del(&code_index_key(&self.account_id, &code)).await?;
        }
        self.del(key).await?;
        self.del(&format!("{key}:attempts")).await
    }
}

// ── UserBindingStore implementation (DB-backed) ────────────────────────────

/// Database-backed [`UserBindingStore`] using the `wx_openid` column.
pub struct DbUserBindingStore {
    db: Arc<AutoRouter>,
}

impl DbUserBindingStore {
    pub fn new(db: Arc<AutoRouter>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl UserBindingStore for DbUserBindingStore {
    type UserId = i64;

    async fn find_user_by_openid(
        &self,
        openid: &str,
    ) -> wechat_api::WechatResult<Option<Self::UserId>> {
        use crate::repositories::user::{Column, Entity as UserEntity};

        let user = UserEntity::find()
            .filter(Column::WxOpenid.eq(openid))
            .one(self.db.write_conn())
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!(
                    "DB query by wx_openid failed: {e}"
                ))
            })?;

        Ok(user.map(|u| u.id))
    }

    async fn bind_openid(
        &self,
        user_id: &Self::UserId,
        openid: &str,
    ) -> wechat_api::WechatResult<()> {
        let sql = "UPDATE users SET wx_openid = $1 WHERE id = $2";
        self.db
            .write_conn()
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                [openid.into(), (*user_id).into()],
            ))
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!("Failed to bind wx_openid: {e}"))
            })?;

        Ok(())
    }

    async fn unbind_openid(&self, user_id: &Self::UserId) -> wechat_api::WechatResult<()> {
        let sql = "UPDATE users SET wx_openid = NULL WHERE id = $1";
        self.db
            .write_conn()
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                [(*user_id).into()],
            ))
            .await
            .map_err(|e| {
                wechat_api::WechatError::Internal(anyhow::anyhow!(
                    "Failed to unbind wx_openid: {e}"
                ))
            })?;

        Ok(())
    }
}

// ── WechatComponents bundle ────────────────────────────────────────────────

#[derive(Clone)]
/// Bundled WeChat services that are injected into `AppState` when enabled.
pub struct WechatComponents {
    /// Raw WeChat configuration (app_id, account_id, etc.).
    pub config: WechatConfig,
    /// Captcha service (generate / verify).
    pub captcha_service: Arc<CaptchaService>,
    /// Login service orchestration.
    pub login_service: Arc<LoginService<DbUserBindingStore>>,
    /// Reference to the captcha store for reverse-index lookups (code → openid).
    pub captcha_store: Arc<RedisCaptchaStore>,
}

/// Initialise WeChat components from configuration.
///
/// Returns `None` when `config.enabled` is `false`, so the caller can
/// conditionally register routes.
pub fn init_wechat_components(
    config: &WechatAccountConfig,
    cache: &CacheService,
    db: Arc<AutoRouter>,
) -> Option<WechatComponents> {
    if !config.enabled {
        tracing::info!("WeChat captcha-login is disabled (set [wechat].enabled = true to enable)");
        return None;
    }

    // Validate required credentials.
    if config.app_id.is_empty() || config.app_secret.is_empty() || config.token.is_empty() {
        tracing::warn!(
            "WeChat captcha-login is enabled but app_id/app_secret/token are not fully configured — feature disabled"
        );
        return None;
    }

    let redis_client = cache.redis_client();

    // Build the captcha store (Redis).
    let captcha_store = Arc::new(RedisCaptchaStore::new(
        if config.account_id.is_empty() {
            "default"
        } else {
            &config.account_id
        },
        redis_client,
    ));

    // Build CaptchaService with login config.
    let login_config = LoginConfig {
        captcha_ttl_secs: config.captcha_ttl_secs,
        resend_cooldown_secs: config.resend_cooldown_secs,
        max_failed_attempts: config.max_failed_attempts,
        captcha_len: config.captcha_len,
        trigger_keywords: config.trigger_keywords.clone(),
    };
    let captcha_service = Arc::new(CaptchaService::new(captcha_store.clone(), login_config));

    // Build UserBindingStore (DB).
    let binding_store = Arc::new(DbUserBindingStore::new(db));

    // Build LoginService.
    let login_service = Arc::new(LoginService::new(captcha_service.clone(), binding_store));

    // Build WechatConfig for callbacks / client.
    let message_mode = match config.message_mode.to_lowercase().as_str() {
        "safe" => wechat_api::MessageMode::Safe,
        "compatible" => wechat_api::MessageMode::Compatible,
        _ => wechat_api::MessageMode::Plain,
    };
    let wechat_config = WechatConfig {
        account_id: if config.account_id.is_empty() {
            "default".to_string()
        } else {
            config.account_id.clone()
        },
        app_id: config.app_id.clone(),
        app_secret: config.app_secret.clone(),
        token: config.token.clone(),
        encoding_aes_key: config.encoding_aes_key.clone(),
        original_id: config.original_id.clone(),
        message_mode,
    };

    tracing::info!(
        "WeChat captcha-login enabled (account: {})",
        wechat_config.account_id
    );

    Some(WechatComponents {
        config: wechat_config,
        captcha_service,
        login_service,
        captcha_store,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::CacheService;
    use std::sync::Arc;

    // ── Redis-dependent tests ─────────────────────────────────────────────
    // Run with: cargo test -p webshelf-server -- --ignored
    //
    // CI has Redis on 127.0.0.1:6379 (no password).
    // Local: docker run -d -p 6379:6379 redis:7-alpine

    fn redis_url() -> &'static str {
        "redis://127.0.0.1:6379"
    }

    fn account_id() -> &'static str {
        "test_acct"
    }

    async fn create_store() -> (CacheService, Arc<RedisCaptchaStore>) {
        let cache = CacheService::new(redis_url(), 10).await;
        let store = Arc::new(RedisCaptchaStore::new(account_id(), cache.redis_client()));
        (cache, store)
    }

    fn captcha_key(openid: &str) -> String {
        format!("wechat:{}:captcha:{}", account_id(), openid)
    }

    fn code_index_key(code: &str) -> String {
        format!("wechat:{}:code:{}", account_id(), code)
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_set_and_consume() {
        let (_cache, store) = create_store().await;
        let openid = "test_set_and_consume";
        let key = captcha_key(openid);
        let code = "AB123";

        store
            .set_captcha(&key, code, openid, Duration::from_secs(60))
            .await
            .unwrap();

        // First consume returns the value.
        let consumed = store.consume_captcha(&key).await.unwrap();
        assert_eq!(consumed, Some((code.to_string(), openid.to_string())));

        // Second consume returns None (one-shot).
        let again = store.consume_captcha(&key).await.unwrap();
        assert!(again.is_none(), "captcha must be one-shot");
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_peek() {
        let (_cache, store) = create_store().await;
        let openid = "test_peek";
        let key = captcha_key(openid);
        let code = "XY789";

        store
            .set_captcha(&key, code, openid, Duration::from_secs(60))
            .await
            .unwrap();

        // Peek without consuming.
        let peeked = store.peek_captcha(&key).await.unwrap();
        assert_eq!(peeked, Some((code.to_string(), openid.to_string())));

        // After consume, peek returns None.
        store.consume_captcha(&key).await.unwrap();
        let after = store.peek_captcha(&key).await.unwrap();
        assert!(after.is_none(), "peek after consume must return None");
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_attempts() {
        let (_cache, store) = create_store().await;
        let key = captcha_key("test_attempts");

        assert_eq!(store.incr_failed_attempts(&key).await.unwrap(), 1);
        assert_eq!(store.incr_failed_attempts(&key).await.unwrap(), 2);
        assert_eq!(store.incr_failed_attempts(&key).await.unwrap(), 3);

        store.reset_failed_attempts(&key).await.unwrap();
        // After reset, counter starts from 1 again.
        assert_eq!(store.incr_failed_attempts(&key).await.unwrap(), 1);
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_reverse_index() {
        let (_cache, store) = create_store().await;
        let openid = "test_rev_idx";
        let key = captcha_key(openid);
        let code = "RV123";

        store
            .set_captcha(&key, code, openid, Duration::from_secs(60))
            .await
            .unwrap();

        // Verify reverse index exists: code → openid.
        let rev_key = code_index_key(code);
        let reverse = store.get_opt(&rev_key).await.unwrap();
        assert_eq!(reverse, Some(openid.to_string()));

        // Consume the captcha — reverse index must also be cleaned up.
        store.consume_captcha(&key).await.unwrap();

        let after = store.get_opt(&rev_key).await.unwrap();
        assert!(
            after.is_none(),
            "reverse index must be deleted after consume"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_delete() {
        let (_cache, store) = create_store().await;
        let openid = "test_delete";
        let key = captcha_key(openid);
        let code = "DL999";

        store
            .set_captcha(&key, code, openid, Duration::from_secs(60))
            .await
            .unwrap();

        // Captcha exists.
        assert!(store.peek_captcha(&key).await.unwrap().is_some());

        // Delete it.
        store.delete_captcha(&key).await.unwrap();

        // Verify gone.
        assert!(store.peek_captcha(&key).await.unwrap().is_none());
        assert!(store.consume_captcha(&key).await.unwrap().is_none());

        // Reverse index must also be deleted.
        let rev_key = code_index_key(code);
        let reverse = store.get_opt(&rev_key).await.unwrap();
        assert!(
            reverse.is_none(),
            "reverse index must be deleted after delete_captcha"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_consume_atomicity_race() {
        // Verify that concurrent consume_captcha calls are atomic:
        // two tasks both try to consume the same captcha — only one should
        // succeed (the other gets None).
        let (_cache, store) = create_store().await;
        let openid = "test_race";
        let key = captcha_key(openid);
        let code = "RC789";

        store
            .set_captcha(&key, code, openid, Duration::from_secs(60))
            .await
            .unwrap();

        let s1 = Arc::clone(&store);
        let s2 = Arc::clone(&store);
        let key_for_h1 = key.clone();

        let h1 = tokio::spawn(async move { s1.consume_captcha(&key_for_h1).await.unwrap() });
        let h2 = tokio::spawn(async move { s2.consume_captcha(&key).await.unwrap() });

        let (r1, r2) = tokio::join!(h1, h2);
        let r1 = r1.unwrap();
        let r2 = r2.unwrap();

        // Exactly one should succeed, one should get None.
        let successes = [r1.is_some(), r2.is_some()];
        assert_eq!(
            successes.iter().filter(|&&b| b).count(),
            1,
            "exactly one concurrent consume must succeed; got r1={:?}, r2={:?}",
            r1,
            r2
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_attempts_reset_on_set() {
        // set_captcha must reset the failed-attempts counter (trait contract).
        let (_cache, store) = create_store().await;
        let openid = "test_attempts_reset";
        let key = captcha_key(openid);
        let code1 = "ATMP1";
        let code2 = "ATMP2";

        // 1. Set captcha and fail some attempts.
        store
            .set_captcha(&key, code1, openid, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(store.incr_failed_attempts(&key).await.unwrap(), 1);
        assert_eq!(store.incr_failed_attempts(&key).await.unwrap(), 2);
        assert_eq!(store.incr_failed_attempts(&key).await.unwrap(), 3);

        // 2. Set a new captcha for the same key — must reset the counter.
        store
            .set_captcha(&key, code2, openid, Duration::from_secs(60))
            .await
            .unwrap();

        // 3. After reset, counter starts from 1 again.
        assert_eq!(
            store.incr_failed_attempts(&key).await.unwrap(),
            1,
            "failed-attempts counter must be reset after set_captcha"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_captcha_reverse_index_ttl() {
        // Verify that the reverse-index key (code → openid) has its TTL
        // set to match the captcha key's TTL.
        let (_cache, store) = create_store().await;
        let openid = "test_rev_ttl";
        let key = captcha_key(openid);
        let code = "RTTL1";
        let ttl_secs = 180u64;

        store
            .set_captcha(&key, code, openid, Duration::from_secs(ttl_secs))
            .await
            .unwrap();

        let rev_key = code_index_key(code);
        let mut conn = store.conn().await.unwrap();

        let captcha_ttl: i64 = redis::cmd("TTL")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(
            captcha_ttl > 0 && (captcha_ttl as u64) <= ttl_secs,
            "captcha key TTL should be > 0 and <= {ttl_secs}, got {captcha_ttl}"
        );

        let rev_ttl: i64 = redis::cmd("TTL")
            .arg(&rev_key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(
            rev_ttl > 0 && (rev_ttl as u64) <= ttl_secs,
            "reverse-index key TTL should be > 0 and <= {ttl_secs}, got {rev_ttl}"
        );
    }

    #[tokio::test]
    async fn test_redis_store_none_client_graceful_degradation() {
        // When no Redis client is provided (client: None), read-style
        // operations must gracefully no-op (Ok(None) / Ok(())) and
        // write-style operations must return a descriptive error.
        // No operation should panic or hang.
        let store = RedisCaptchaStore::new("degraded", None);
        let key = "degraded:captcha:test";
        let code = "ABCDE";
        let openid = "oDegradeTest";
        let ttl = Duration::from_secs(60);

        // Read-style operations — gracefully no-op.
        assert!(
            store.peek_captcha(key).await.unwrap().is_none(),
            "peek_captcha must return None when Redis is unavailable"
        );
        assert!(
            store.consume_captcha(key).await.unwrap().is_none(),
            "consume_captcha must return None when Redis is unavailable"
        );
        assert!(
            store.delete_captcha(key).await.is_ok(),
            "delete_captcha must return Ok when Redis is unavailable"
        );
        assert!(
            store.reset_failed_attempts(key).await.is_ok(),
            "reset_failed_attempts must return Ok when Redis is unavailable"
        );

        // Write-style operations — return Err with descriptive message.
        assert!(
            store.set_captcha(key, code, openid, ttl).await.is_err(),
            "set_captcha must return Err when Redis is unavailable"
        );
        assert!(
            store.set_captcha_nx(key, code, openid, ttl).await.is_err(),
            "set_captcha_nx must return Err when Redis is unavailable"
        );
        assert!(
            store.incr_failed_attempts(key).await.is_err(),
            "incr_failed_attempts must return Err when Redis is unavailable"
        );
    }
}
