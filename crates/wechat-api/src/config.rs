//! WeChat Official Account configuration.
//!
//! Holds the credentials and runtime settings needed to interact with the
//! WeChat Platform: server signature verification, access-token retrieval,
//! and (optionally) AES message decryption in "safe mode".

use serde::{Deserialize, Serialize};

/// WeChat message encryption mode configured on the MP platform.
///
/// See: <https://developers.weixin.qq.com/doc/offiaccount/Message_Management/Message_Encryption_and_Decryption_Instructions.html>
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageMode {
    /// Plaintext mode — no encryption, no signature on the message body.
    #[default]
    Plain,
    /// Compatibility mode — encrypted body present but plaintext also readable.
    Compatible,
    /// Safe mode — fully encrypted message body (requires `encoding_aes_key`).
    Safe,
}

/// Configuration for a single WeChat Official Account.
///
/// Multiple accounts can be hosted by constructing one `WechatConfig` per
/// account and dispatching incoming callbacks based on the `original_id`
/// found in the XML message body.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WechatConfig {
    /// A logical identifier for this account (e.g. primary tenant id).
    /// Used as a namespace prefix for cache keys so multiple accounts never
    /// collide.
    #[serde(default)]
    pub account_id: String,

    /// WeChat AppID.
    pub app_id: String,

    /// WeChat AppSecret.
    pub app_secret: String,

    /// The Token configured in the MP backend for signature verification.
    pub token: String,

    /// The EncodingAESKey (43 chars) configured for "safe mode".
    /// Only required when [`MessageMode::Safe`] is used.
    #[serde(default)]
    pub encoding_aes_key: Option<String>,

    /// The original ID of the official account (e.g. `gh_xxxx`).
    /// Used to route incoming messages to the correct account config.
    #[serde(default)]
    pub original_id: Option<String>,

    /// Configured message encryption mode.
    #[serde(default)]
    pub message_mode: MessageMode,
}

impl WechatConfig {
    /// Returns `true` when this account is configured for encrypted messages.
    pub fn is_safe_mode(&self) -> bool {
        matches!(self.message_mode, MessageMode::Safe)
    }

    /// Returns `true` when the credentials required for API calls are present.
    pub fn is_api_configured(&self) -> bool {
        !self.app_id.is_empty() && !self.app_secret.is_empty()
    }
}

/// Login-flow specific configuration for the WeChat captcha login feature.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoginConfig {
    /// How long a generated captcha stays valid, in seconds. Default 300s.
    #[serde(default = "default_captcha_ttl")]
    pub captcha_ttl_secs: u64,

    /// Minimum interval between two captcha requests from the same openid,
    /// in seconds. Prevents spamming the official account. Default 60s.
    #[serde(default = "default_resend_cooldown")]
    pub resend_cooldown_secs: u64,

    /// Maximum consecutive failed login attempts before the captcha is
    /// invalidated. Default 5.
    #[serde(default = "default_max_attempts")]
    pub max_failed_attempts: u32,

    /// Length of the generated captcha code. Default 5.
    #[serde(default = "default_captcha_len")]
    pub captcha_len: usize,

    /// Keywords that trigger captcha generation when sent to the official
    /// account by the user. Defaults to `["验证码", "登录码", "login"]`.
    #[serde(default = "default_trigger_keywords")]
    pub trigger_keywords: Vec<String>,
}

impl Default for LoginConfig {
    fn default() -> Self {
        Self {
            captcha_ttl_secs: default_captcha_ttl(),
            resend_cooldown_secs: default_resend_cooldown(),
            max_failed_attempts: default_max_attempts(),
            captcha_len: default_captcha_len(),
            trigger_keywords: default_trigger_keywords(),
        }
    }
}

fn default_captcha_ttl() -> u64 {
    300
}
fn default_resend_cooldown() -> u64 {
    60
}
fn default_max_attempts() -> u32 {
    5
}
fn default_captcha_len() -> usize {
    5
}
fn default_trigger_keywords() -> Vec<String> {
    vec![
        "验证码".to_string(),
        "登录码".to_string(),
        "login".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_mode_default() {
        assert_eq!(MessageMode::default(), MessageMode::Plain);
    }

    #[test]
    fn test_login_config_defaults() {
        let cfg = LoginConfig::default();
        assert_eq!(cfg.captcha_ttl_secs, 300);
        assert_eq!(cfg.resend_cooldown_secs, 60);
        assert_eq!(cfg.max_failed_attempts, 5);
        assert_eq!(cfg.captcha_len, 5);
        assert!(cfg.trigger_keywords.contains(&"验证码".to_string()));
    }

    #[test]
    fn test_wechat_config_is_api_configured() {
        let cfg = WechatConfig {
            account_id: "default".into(),
            app_id: "".into(),
            app_secret: "".into(),
            token: "t".into(),
            encoding_aes_key: None,
            original_id: None,
            message_mode: MessageMode::Plain,
        };
        assert!(!cfg.is_api_configured());

        let cfg2 = WechatConfig {
            account_id: "default".into(),
            app_id: "wx123".into(),
            app_secret: "secret".into(),
            token: "t".into(),
            encoding_aes_key: None,
            original_id: None,
            message_mode: MessageMode::Plain,
        };
        assert!(cfg2.is_api_configured());
    }
}
