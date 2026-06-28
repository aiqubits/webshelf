//! Extensible message-handler trait and a default captcha-trigger implementation.
//!
//! The SDK separates *parsing* (in [`crate::callback`]) from *handling*
//! (here) so that host applications can plug in arbitrary business logic
//! — auto-replies, analytics, captcha generation, etc. — without modifying
//! the SDK core.
//!
//! ## Extension model
//!
//! Implement [`MessageHandler`] for your application's handler. The SDK
//! provides [`CaptchaTriggerHandler`], a ready-to-use handler that listens
//! for captcha-trigger keywords, generates a code via
//! [`crate::captcha::CaptchaService`], and pushes it to the user via
//! [`crate::client::WechatClient`]. Compose it with your own handler using
//! the [`HandlerChain`] wrapper.

use std::sync::Arc;

use async_trait::async_trait;

use crate::config::WechatConfig;
use crate::error::WechatResult;
use crate::message::{self, WechatMessage};

#[cfg(feature = "client")]
use crate::captcha::CaptchaService;
#[cfg(feature = "client")]
use crate::client::WechatClient;
#[cfg(feature = "client")]
use crate::error::WechatError;

/// Outcome of handling a message.
pub enum HandleOutcome {
    /// Reply with this XML string to WeChat.
    Reply(String),
    /// No reply (WeChat expects an empty body or "success").
    NoReply,
}

/// Trait for processing a parsed WeChat message and producing a reply.
///
/// Implementations are `Send + Sync` so they can be shared across async
/// tasks. Multiple handlers can be chained via [`HandlerChain`].
#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle(
        &self,
        config: &WechatConfig,
        msg: &WechatMessage,
    ) -> WechatResult<HandleOutcome>;
}

/// A chain of handlers executed in order. The first handler that returns
/// `Reply` short-circuits the chain.
pub struct HandlerChain {
    handlers: Vec<Arc<dyn MessageHandler>>,
}

impl HandlerChain {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn with<H: MessageHandler + 'static>(mut self, handler: H) -> Self {
        self.handlers.push(Arc::new(handler));
        self
    }

    pub fn push<H: MessageHandler + 'static>(&mut self, handler: H) {
        self.handlers.push(Arc::new(handler));
    }
}

impl Default for HandlerChain {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageHandler for HandlerChain {
    async fn handle(
        &self,
        config: &WechatConfig,
        msg: &WechatMessage,
    ) -> WechatResult<HandleOutcome> {
        for h in &self.handlers {
            match h.handle(config, msg).await? {
                HandleOutcome::Reply(xml) => return Ok(HandleOutcome::Reply(xml)),
                HandleOutcome::NoReply => continue,
            }
        }
        Ok(HandleOutcome::NoReply)
    }
}

// ── Default handler: captcha trigger ────────────────────────────────────────

/// A handler that listens for captcha-trigger keywords and pushes a freshly
/// generated captcha to the user via a WeChat customer-service message.
///
/// On a trigger match it:
/// 1. Generates a captcha via [`CaptchaService::generate`].
/// 2. Sends it to the user via [`WechatClient::send_text_message`].
/// 3. Replies with a short confirmation XML (so WeChat doesn't retry).
///
/// If a cooldown is active ([`WechatError::CooldownActive`]), it replies
/// with a "please wait" message instead of erroring.
#[cfg(feature = "client")]
pub struct CaptchaTriggerHandler {
    captcha: Arc<CaptchaService>,
    client: Arc<WechatClient>,
}

#[cfg(feature = "client")]
impl CaptchaTriggerHandler {
    pub fn new(captcha: Arc<CaptchaService>, client: Arc<WechatClient>) -> Self {
        Self { captcha, client }
    }
}

#[cfg(feature = "client")]
#[async_trait]
impl MessageHandler for CaptchaTriggerHandler {
    async fn handle(
        &self,
        config: &WechatConfig,
        msg: &WechatMessage,
    ) -> WechatResult<HandleOutcome> {
        // Only react to text messages matching a trigger keyword.
        if !msg.is_text() || !self.captcha.matches_trigger(msg.text_content()) {
            return Ok(HandleOutcome::NoReply);
        }

        let openid = &msg.from_user_name;
        let account_id = &config.account_id;

        match self.captcha.generate(account_id, openid).await {
            Ok(code) => {
                let content = format!("您的登录验证码为：{code}，有效期 5 分钟，请尽快使用。");
                // Best-effort send via customer-service message. If the API
                // call fails we still reply with the code inline so the user
                // isn't left without a captcha (the official-account passive
                // reply has a 48-hour window limitation, but for a freshly
                // messaged user it works).
                if let Err(e) = self.client.send_text_message(openid, &content).await {
                    tracing::warn!(error = %e, "Failed to send captcha via custom message; replying inline");
                    let reply = message::build_text_reply(openid, &msg.to_user_name, &content);
                    return Ok(HandleOutcome::Reply(reply));
                }
                let reply = message::build_text_reply(
                    openid,
                    &msg.to_user_name,
                    "验证码已通过公众号消息发送给您，请注意查收。",
                );
                Ok(HandleOutcome::Reply(reply))
            }
            Err(WechatError::CooldownActive) => {
                let reply = message::build_text_reply(
                    openid,
                    &msg.to_user_name,
                    "验证码已发送，请稍候再试。",
                );
                Ok(HandleOutcome::Reply(reply))
            }
            Err(e) => Err(e),
        }
    }
}

/// A simple fallback handler that replies with a fixed welcome message for
/// subscribe events and a default text for unmatched messages.
pub struct DefaultReplyHandler {
    pub welcome: String,
    pub fallback: String,
}

impl DefaultReplyHandler {
    pub fn new(welcome: impl Into<String>, fallback: impl Into<String>) -> Self {
        Self {
            welcome: welcome.into(),
            fallback: fallback.into(),
        }
    }
}

impl Default for DefaultReplyHandler {
    fn default() -> Self {
        Self::new("欢迎关注！", "感谢您的留言。")
    }
}

#[async_trait]
impl MessageHandler for DefaultReplyHandler {
    async fn handle(
        &self,
        _config: &WechatConfig,
        msg: &WechatMessage,
    ) -> WechatResult<HandleOutcome> {
        if msg.is_subscribe() {
            return Ok(HandleOutcome::Reply(message::build_text_reply(
                &msg.from_user_name,
                &msg.to_user_name,
                &self.welcome,
            )));
        }
        if msg.is_text() || msg.is_event() {
            return Ok(HandleOutcome::Reply(message::build_text_reply(
                &msg.from_user_name,
                &msg.to_user_name,
                &self.fallback,
            )));
        }
        Ok(HandleOutcome::NoReply)
    }
}

#[cfg(test)]
#[cfg(all(feature = "memory-store", feature = "client"))]
mod tests {
    use super::*;
    use crate::captcha::CaptchaService;
    use crate::client::WechatClient;
    use crate::config::{LoginConfig, MessageMode, WechatConfig};
    use crate::store::CaptchaStore;
    use crate::store::memory::{MemoryAccessTokenStore, MemoryCaptchaStore};

    fn config() -> WechatConfig {
        WechatConfig {
            account_id: "test".into(),
            app_id: "wx".into(),
            app_secret: "s".into(),
            token: "t".into(),
            encoding_aes_key: None,
            original_id: Some("gh".into()),
            message_mode: MessageMode::Plain,
        }
    }

    fn text_msg(content: &str) -> WechatMessage {
        WechatMessage {
            to_user_name: "gh".into(),
            from_user_name: "oUser".into(),
            create_time: 0,
            msg_type: "text".into(),
            content: Some(content.into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_handler_chain_first_reply_wins() {
        struct AlwaysReply(&'static str);
        #[async_trait]
        impl MessageHandler for AlwaysReply {
            async fn handle(
                &self,
                _c: &WechatConfig,
                _m: &WechatMessage,
            ) -> WechatResult<HandleOutcome> {
                Ok(HandleOutcome::Reply(self.0.into()))
            }
        }

        let chain = HandlerChain::new()
            .with(AlwaysReply("first"))
            .with(AlwaysReply("second"));

        let cfg = config();
        let msg = text_msg("hi");
        match chain.handle(&cfg, &msg).await.unwrap() {
            HandleOutcome::Reply(xml) => assert!(xml.contains("first")),
            HandleOutcome::NoReply => panic!("expected reply"),
        }
    }

    #[tokio::test]
    async fn test_handler_chain_no_reply_when_all_pass() {
        struct Pass;
        #[async_trait]
        impl MessageHandler for Pass {
            async fn handle(
                &self,
                _c: &WechatConfig,
                _m: &WechatMessage,
            ) -> WechatResult<HandleOutcome> {
                Ok(HandleOutcome::NoReply)
            }
        }

        let chain = HandlerChain::new().with(Pass).with(Pass);
        let cfg = config();
        let msg = text_msg("hi");
        assert!(matches!(
            chain.handle(&cfg, &msg).await.unwrap(),
            HandleOutcome::NoReply
        ));
    }

    #[tokio::test]
    async fn test_default_reply_handler_subscribe() {
        let h = DefaultReplyHandler::default();
        let cfg = config();
        let msg = WechatMessage {
            msg_type: "event".into(),
            event: Some("subscribe".into()),
            from_user_name: "oUser".into(),
            to_user_name: "gh".into(),
            ..Default::default()
        };
        match h.handle(&cfg, &msg).await.unwrap() {
            HandleOutcome::Reply(xml) => assert!(xml.contains("欢迎关注")),
            HandleOutcome::NoReply => panic!("expected reply"),
        }
    }

    #[tokio::test]
    async fn test_default_reply_handler_unmatched_returns_no_reply() {
        let h = DefaultReplyHandler::default();
        let cfg = config();
        let msg = WechatMessage {
            msg_type: "image".into(),
            from_user_name: "oUser".into(),
            to_user_name: "gh".into(),
            ..Default::default()
        };
        assert!(matches!(
            h.handle(&cfg, &msg).await.unwrap(),
            HandleOutcome::NoReply
        ));
    }

    #[tokio::test]
    async fn test_captcha_trigger_handler_generates_on_keyword() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let token_store = Arc::new(MemoryAccessTokenStore::new());
        let captcha = Arc::new(CaptchaService::new(store.clone(), LoginConfig::default()));
        let client = Arc::new(WechatClient::new(config(), token_store));
        let handler = CaptchaTriggerHandler::new(captcha.clone(), client);

        let cfg = config();
        let msg = text_msg("验证码");
        let outcome = handler.handle(&cfg, &msg).await.unwrap();
        match outcome {
            HandleOutcome::Reply(xml) => {
                // Should contain either the inline code or the "sent" notice.
                assert!(xml.contains("验证码") || xml.contains("已发送"));
            }
            HandleOutcome::NoReply => panic!("expected reply for trigger keyword"),
        }

        // The captcha should now be stored.
        let key = captcha.captcha_key(&cfg.account_id, "oUser");
        assert!(store.peek_captcha(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_captcha_trigger_handler_ignores_non_keyword() {
        let store = Arc::new(MemoryCaptchaStore::new());
        let token_store = Arc::new(MemoryAccessTokenStore::new());
        let captcha = Arc::new(CaptchaService::new(store, LoginConfig::default()));
        let client = Arc::new(WechatClient::new(config(), token_store));
        let handler = CaptchaTriggerHandler::new(captcha, client);

        let cfg = config();
        let msg = text_msg("hello world");
        assert!(matches!(
            handler.handle(&cfg, &msg).await.unwrap(),
            HandleOutcome::NoReply
        ));
    }
}
