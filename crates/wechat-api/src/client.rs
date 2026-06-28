//! WeChat Platform HTTP API client.
//!
//! Wraps [`reqwest`] to call the WeChat Official Account APIs:
//! - `access_token` retrieval (with caching via [`AccessTokenStore`])
//! - Customer-service text message sending (used to push captcha codes)
//!
//! This module is behind the `client` feature flag so the SDK can be used
//! in callback-only mode without pulling in an HTTP client.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::config::WechatConfig;
use crate::error::{WechatError, WechatResult};
use crate::store::AccessTokenStore;

/// Trait abstraction over the HTTP transport so the client can be tested
/// without making real network calls.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> WechatResult<serde_json::Value>;
    async fn get_json(&self, url: &str) -> WechatResult<serde_json::Value>;
}

/// Default HTTP transport backed by `reqwest`.
#[derive(Clone)]
pub struct ReqwestTransport {
    inner: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        let inner = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");
        Self { inner }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> WechatResult<serde_json::Value> {
        let resp = self
            .inner
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| WechatError::ApiRequest(e.to_string()))?;
        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| WechatError::ApiRequest(e.to_string()))
    }

    async fn get_json(&self, url: &str) -> WechatResult<serde_json::Value> {
        let resp = self
            .inner
            .get(url)
            .send()
            .await
            .map_err(|e| WechatError::ApiRequest(e.to_string()))?;
        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| WechatError::ApiRequest(e.to_string()))
    }
}

const TOKEN_API: &str = "https://api.weixin.qq.com/cgi-bin/token";
const CUSTOM_MESSAGE_API: &str = "https://api.weixin.qq.com/cgi-bin/message/custom/send";

/// Response from the `cgi-bin/token` endpoint.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    expires_in: Option<u64>,
    errcode: Option<i64>,
    errmsg: Option<String>,
}

/// WeChat Platform API client.
///
/// Holds the account config, an HTTP transport, and a token cache. The
/// client is cheap to clone (all fields are `Arc` / `Clone`).
pub struct WechatClient {
    config: WechatConfig,
    transport: Arc<dyn HttpTransport>,
    token_store: Arc<dyn AccessTokenStore>,
}

impl WechatClient {
    /// Create a new client with a custom transport (useful for testing).
    pub fn with_transport(
        config: WechatConfig,
        transport: Arc<dyn HttpTransport>,
        token_store: Arc<dyn AccessTokenStore>,
    ) -> Self {
        Self {
            config,
            transport,
            token_store,
        }
    }

    /// Create a new client using the default `reqwest` transport.
    pub fn new(config: WechatConfig, token_store: Arc<dyn AccessTokenStore>) -> Self {
        Self::with_transport(config, Arc::new(ReqwestTransport::new()), token_store)
    }

    /// Retrieve a valid access token, refreshing from the WeChat API if
    /// the cached token is missing or expired.
    pub async fn access_token(&self) -> WechatResult<String> {
        // 1. Try cache
        if let Some(token) = self
            .token_store
            .get_access_token(&self.config.app_id)
            .await?
        {
            return Ok(token);
        }

        // 2. Fetch from WeChat
        let url = format!(
            "{TOKEN_API}?grant_type=client_credential&appid={}&secret={}",
            self.config.app_id, self.config.app_secret
        );
        let resp: serde_json::Value = self.transport.get_json(&url).await?;
        let parsed: TokenResponse = serde_json::from_value(resp)
            .map_err(|e| WechatError::ApiRequest(format!("token parse: {e}")))?;

        if let Some(code) = parsed.errcode
            && code != 0
        {
            return Err(WechatError::ApiBusiness {
                errcode: code,
                errmsg: parsed.errmsg.unwrap_or_default(),
            });
        }

        let token = parsed
            .access_token
            .ok_or_else(|| WechatError::ApiRequest("missing access_token in response".into()))?;
        let expires_in = parsed.expires_in.unwrap_or(7200);

        // Cache with a small safety margin (refresh 5 minutes early).
        let ttl = Duration::from_secs(expires_in.saturating_sub(300).max(60));
        self.token_store
            .set_access_token(&self.config.app_id, &token, ttl)
            .await?;

        Ok(token)
    }

    /// Send a customer-service text message to a user identified by `openid`.
    ///
    /// This is the primary mechanism for pushing a captcha code to the
    /// user's WeChat client after they send a trigger keyword.
    pub async fn send_text_message(&self, openid: &str, content: &str) -> WechatResult<()> {
        let token = self.access_token().await?;
        let url = format!("{CUSTOM_MESSAGE_API}?access_token={token}");
        let body = serde_json::json!({
            "touser": openid,
            "msgtype": "text",
            "text": { "content": content }
        });

        let resp = self.transport.post_json(&url, &body).await?;
        check_api_error(&resp)?;
        tracing::info!(openid = %openid, "WeChat custom text message sent");
        Ok(())
    }
}

/// Check a WeChat API response for a non-zero `errcode`.
fn check_api_error(resp: &serde_json::Value) -> WechatResult<()> {
    if let Some(code) = resp.get("errcode").and_then(|v| v.as_i64())
        && code != 0
    {
        let errmsg = resp
            .get("errmsg")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Err(WechatError::ApiBusiness {
            errcode: code,
            errmsg,
        });
    }
    Ok(())
}

#[cfg(test)]
#[cfg(feature = "memory-store")]
mod tests {
    use super::*;
    use crate::store::memory::MemoryAccessTokenStore;
    use std::sync::Mutex;

    /// A mock transport that returns canned responses for testing.
    struct MockTransport {
        responses: Mutex<Vec<serde_json::Value>>,
    }

    impl MockTransport {
        fn new(responses: Vec<serde_json::Value>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl HttpTransport for MockTransport {
        async fn post_json(
            &self,
            _url: &str,
            _body: &serde_json::Value,
        ) -> WechatResult<serde_json::Value> {
            self.responses
                .lock()
                .unwrap()
                .pop()
                .ok_or_else(|| WechatError::ApiRequest("no more mock responses".into()))
        }

        async fn get_json(&self, _url: &str) -> WechatResult<serde_json::Value> {
            self.responses
                .lock()
                .unwrap()
                .pop()
                .ok_or_else(|| WechatError::ApiRequest("no more mock responses".into()))
        }
    }

    fn test_config() -> WechatConfig {
        WechatConfig {
            account_id: "test".into(),
            app_id: "wx123".into(),
            app_secret: "secret".into(),
            token: "tok".into(),
            encoding_aes_key: None,
            original_id: Some("gh_test".into()),
            message_mode: crate::config::MessageMode::Plain,
        }
    }

    #[tokio::test]
    async fn test_access_token_caches_and_reuses() {
        // The mock returns a token on the first GET; the second call should
        // hit the cache and not call the transport again (we only provide
        // one mock response).
        let mock = Arc::new(MockTransport::new(vec![
            serde_json::json!({ "access_token": "tk_1", "expires_in": 7200 }),
        ]));
        let store = Arc::new(MemoryAccessTokenStore::new());
        let client = WechatClient::with_transport(test_config(), mock, store);

        let t1 = client.access_token().await.unwrap();
        assert_eq!(t1, "tk_1");

        // Second call: cache hit (no transport call needed).
        let t2 = client.access_token().await.unwrap();
        assert_eq!(t2, "tk_1");
    }

    #[tokio::test]
    async fn test_access_token_api_error() {
        let mock = Arc::new(MockTransport::new(vec![
            serde_json::json!({ "errcode": 40013, "errmsg": "invalid appid" }),
        ]));
        let store = Arc::new(MemoryAccessTokenStore::new());
        let client = WechatClient::with_transport(test_config(), mock, store);

        let err = client.access_token().await.unwrap_err();
        assert!(matches!(
            err,
            WechatError::ApiBusiness { errcode: 40013, .. }
        ));
    }

    #[tokio::test]
    async fn test_send_text_message_success() {
        // Two responses: token (GET) then send result (POST).
        let mock = Arc::new(MockTransport::new(vec![
            serde_json::json!({ "errcode": 0, "errmsg": "ok" }),
            serde_json::json!({ "access_token": "tk_send", "expires_in": 7200 }),
        ]));
        let store = Arc::new(MemoryAccessTokenStore::new());
        let client = WechatClient::with_transport(test_config(), mock, store);

        client
            .send_text_message("oUser", "Your code: 12345")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_send_text_message_business_error() {
        let mock = Arc::new(MockTransport::new(vec![
            serde_json::json!({ "errcode": 40001, "errmsg": "invalid credential" }),
            serde_json::json!({ "access_token": "tk_send", "expires_in": 7200 }),
        ]));
        let store = Arc::new(MemoryAccessTokenStore::new());
        let client = WechatClient::with_transport(test_config(), mock, store);

        let err = client.send_text_message("oUser", "code").await.unwrap_err();
        assert!(matches!(
            err,
            WechatError::ApiBusiness { errcode: 40001, .. }
        ));
    }

    #[test]
    fn test_check_api_error_zero_code() {
        let resp = serde_json::json!({ "errcode": 0, "errmsg": "ok" });
        assert!(check_api_error(&resp).is_ok());
    }

    #[test]
    fn test_check_api_error_no_errcode() {
        let resp = serde_json::json!({ "access_token": "abc" });
        assert!(check_api_error(&resp).is_ok());
    }
}
