//! # wechat-api
//!
//! Extensible WeChat Official Account SDK for **webshelf**.
//!
//! This crate encapsulates the WeChat captcha-login flow analyzed,
//! but is designed as a general-purpose, pluggable
//! SDK so future WeChat features (payment, mini-programs, template
//! messages, etc.) can be added as additional modules without refactoring.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                  Host Application                    │
//! │  (webshelf-server / any axum/salvo app)              │
//! │                                                      │
//! │  ┌──────────────┐   implements   ┌────────────────┐  │
//! │  │ CacheService │ ─────────────► │ CaptchaStore   │  │
//! │  │ / DB layers  │               │ UserBindingStore│ │
//! │  └──────────────┘               │ AccessTokenStore│  │
//! │                                  └───────┬────────┘  │
//! └──────────────────────────────────────────┼──────────┘
//!                                            │
//! ┌──────────────────────────────────────────┼──────────┐
//! │                  wechat-api              ▼          │
//! │                                                       │
//! │  config ── crypto ── message ── callback              │
//! │            │                         │                │
//! │            ▼                         ▼                │
//! │  ┌──────────────┐         ┌──────────────────┐        │
//! │  │ CaptchaService│        │  MessageHandler  │        │
//! │  │ (gen/verify)  │        │  (trait + chain) │        │
//! │  └──────┬───────┘         └────────┬─────────┘        │
//! │         │                          │                  │
//! │         ▼                          ▼                  │
//! │  ┌──────────────┐         ┌──────────────────┐        │
//! │  │ LoginService  │        │  WechatClient    │        │
//! │  │ (orchestrate) │        │  (Platform API)  │        │
//! │  └──────────────┘         └──────────────────┘        │
//! └───────────────────────────────────────────────────────┘
//! ```
//!
//! ## Feature flags
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `client` (default) | HTTP client for WeChat Platform API calls (`access_token`, customer-service messages) |
//! | `memory-store` | In-memory implementations of the store traits (tests / single-instance dev) |
//! | `crypto-safe-mode` | AES-CBC decryption for WeChat "safe mode" encrypted messages |
//!
//! ## Quick start: webshelf integration
//!
//! 1. Implement the store traits for webshelf's `CacheService` and DB layer:
//!    ```rust,ignore
//!    use wechat_api::store::{CaptchaStore, UserBindingStore, AccessTokenStore};
//!
//!    #[async_trait]
//!    impl CaptchaStore for webshelf_server::services::CacheService { /* ... */ }
//!    ```
//!
//! 2. Build a `WechatClient` and `CaptchaService`:
//!    ```rust,ignore
//!    let client = Arc::new(WechatClient::new(config, token_store));
//!    let captcha = Arc::new(CaptchaService::new(captcha_store, login_config));
//!    ```
//!
//! 3. Wire the callback handler into a route:
//!    ```rust,ignore
//!    let handler = HandlerChain::new()
//!        .with(CaptchaTriggerHandler::new(captcha.clone(), client.clone()))
//!        .with(DefaultReplyHandler::default());
//!    ```
//!
//! 4. Call `LoginService::verify_and_login` from the `/auth/wx-login` endpoint.
//!
//! ## Extensibility
//!
//! Adding new WeChat capabilities later only requires:
//! - A new module (e.g. `pay.rs`, `template.rs`, `miniprogram/`).
//! - Optionally new store traits if the feature needs persistence.
//! - New `MessageHandler` implementations for callback-driven features.
//!
//! The existing core (`config`, `crypto`, `message`, `callback`) is reused
//! unchanged.

pub mod callback;
pub mod captcha;
#[cfg(feature = "client")]
pub mod client;
pub mod config;
pub mod crypto;
pub mod error;
pub mod handler;
pub mod login;
pub mod message;
pub mod store;

// ── Convenience re-exports ──────────────────────────────────────────────────

pub use callback::{CallbackQuery, ParsedCallback, handle_verification, parse_callback};
pub use captcha::CaptchaService;
pub use config::{LoginConfig, MessageMode, WechatConfig};
pub use error::{WechatError, WechatResult};
#[cfg(feature = "client")]
pub use handler::CaptchaTriggerHandler;
pub use handler::{DefaultReplyHandler, HandleOutcome, HandlerChain, MessageHandler};
pub use login::{LoginService, VerifiedLogin};
pub use message::{
    BasicMessage, WechatMessage, build_image_reply, build_news_reply, build_text_reply,
    parse_basic, parse_message,
};
pub use store::{CaptchaStore, UserBindingStore};

#[cfg(feature = "client")]
pub use store::AccessTokenStore;

#[cfg(feature = "client")]
pub use client::{HttpTransport, ReqwestTransport, WechatClient};

#[cfg(feature = "crypto-safe-mode")]
pub use crypto::aes_crypto;
