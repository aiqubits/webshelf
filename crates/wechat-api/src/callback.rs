//! Callback handling for WeChat Official Account server callbacks.
//!
//! WeChat sends two kinds of requests to the configured callback URL:
//!
//! 1. **GET** — server-URL verification. WeChat sends `signature`,
//!    `timestamp`, `nonce`, `echostr`; the server must verify the signature
//!    and return `echostr` verbatim.
//! 2. **POST** — message / event notification. The body is XML; the server
//!    verifies the signature, parses (and optionally decrypts) the message,
//!    and returns an XML reply.
//!
//! This module provides framework-agnostic functions that take raw inputs
//! and return reply strings, so they can be wired into any web framework
//! (axum, salvo, actix, etc.).

use crate::config::{MessageMode, WechatConfig};
use crate::crypto::verify_signature;
use crate::error::{WechatError, WechatResult};
use crate::message::{self, WechatMessage};

/// Query parameters extracted from a WeChat callback request.
#[derive(Debug, Clone, Default)]
pub struct CallbackQuery {
    pub signature: String,
    pub timestamp: String,
    pub nonce: String,
    /// Only present on GET verification requests.
    pub echostr: Option<String>,
    /// Only present on POST message requests (in plain / compatible mode).
    pub openid: Option<String>,
}

impl CallbackQuery {
    /// Build from a flat key→value map (e.g. axum `Query<HashMap>`).
    pub fn from_params<'a, I>(params: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut q = Self::default();
        for (k, v) in params {
            match k {
                "signature" => q.signature = v.to_string(),
                "timestamp" => q.timestamp = v.to_string(),
                "nonce" => q.nonce = v.to_string(),
                "echostr" => q.echostr = Some(v.to_string()),
                "openid" => q.openid = Some(v.to_string()),
                _ => {}
            }
        }
        q
    }
}

/// Handle a GET verification request.
///
/// Returns the `echostr` to echo back if the signature is valid, or an error.
pub fn handle_verification(config: &WechatConfig, query: &CallbackQuery) -> WechatResult<String> {
    if !verify_signature(
        &config.token,
        &query.timestamp,
        &query.nonce,
        &query.signature,
    ) {
        return Err(WechatError::SignatureMismatch);
    }
    query.echostr.clone().ok_or_else(|| {
        WechatError::Internal(anyhow::anyhow!("missing echostr in verification request"))
    })
}

/// Result of parsing an incoming POST callback.
pub struct ParsedCallback {
    /// The decoded WeChat message.
    pub message: WechatMessage,
}

/// Parse a POST message callback: verify signature, optionally decrypt,
/// and parse the XML body into a [`WechatMessage`].
///
/// Returns the parsed message; the caller is then responsible for invoking
/// a [`crate::handler::MessageHandler`] to produce a reply.
pub fn parse_callback(
    config: &WechatConfig,
    query: &CallbackQuery,
    body: &str,
) -> WechatResult<ParsedCallback> {
    // 1. Signature check
    if !verify_signature(
        &config.token,
        &query.timestamp,
        &query.nonce,
        &query.signature,
    ) {
        return Err(WechatError::SignatureMismatch);
    }

    // 2. Decrypt if safe mode
    let xml = if config.is_safe_mode() {
        #[cfg(feature = "crypto-safe-mode")]
        {
            let aes_key = config.encoding_aes_key.as_deref().ok_or_else(|| {
                WechatError::ConfigIncomplete("encoding_aes_key required for safe mode".into())
            })?;
            // The POST body in safe mode is an XML envelope with an <Encrypt> tag.
            // Extract it, decrypt, and return the inner XML.
            let encrypt = extract_encrypt_tag(body)?;
            crate::crypto::aes_crypto::decrypt_message(aes_key, &encrypt)?
        }
        #[cfg(not(feature = "crypto-safe-mode"))]
        {
            return Err(WechatError::ConfigIncomplete(
                "safe mode requested but crypto-safe-mode feature is not enabled".into(),
            ));
        }
    } else {
        // Plain or compatible mode: body is already plaintext XML.
        body.to_string()
    };

    // 3. Parse
    let message = message::parse_message(&xml)?;
    Ok(ParsedCallback { message })
}

/// Extract the `<Encrypt>` tag value from a safe-mode envelope XML.
#[cfg(feature = "crypto-safe-mode")]
fn extract_encrypt_tag(xml: &str) -> WechatResult<String> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename = "xml")]
    struct EncryptEnvelope {
        #[serde(rename = "Encrypt", default)]
        encrypt: Option<String>,
    }

    let env: EncryptEnvelope = quick_xml::de::from_str(xml)
        .map_err(|e| WechatError::XmlParse(format!("encrypt envelope: {e}")))?;
    env.encrypt
        .ok_or_else(|| WechatError::Decrypt("missing <Encrypt> tag in safe-mode envelope".into()))
}

/// Check whether a `MessageMode` requires the crypto-safe-mode feature.
pub fn requires_safe_mode_feature(config: &WechatConfig) -> bool {
    matches!(config.message_mode, MessageMode::Safe) && !cfg!(feature = "crypto-safe-mode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::compute_signature;

    fn test_config() -> WechatConfig {
        WechatConfig {
            account_id: "test".into(),
            app_id: "wx123".into(),
            app_secret: "secret".into(),
            token: "mytoken".into(),
            encoding_aes_key: None,
            original_id: Some("gh_test".into()),
            message_mode: MessageMode::Plain,
        }
    }

    fn signed_query(
        config: &WechatConfig,
        ts: &str,
        nonce: &str,
        echostr: Option<&str>,
    ) -> CallbackQuery {
        let sig = compute_signature(&config.token, ts, nonce);
        CallbackQuery {
            signature: sig,
            timestamp: ts.to_string(),
            nonce: nonce.to_string(),
            echostr: echostr.map(|s| s.to_string()),
            openid: None,
        }
    }

    #[test]
    fn test_handle_verification_success() {
        let config = test_config();
        let query = signed_query(&config, "1609459200", "nonce123", Some("echo_me"));
        let result = handle_verification(&config, &query).unwrap();
        assert_eq!(result, "echo_me");
    }

    #[test]
    fn test_handle_verification_bad_signature() {
        let config = test_config();
        let query = CallbackQuery {
            signature: "badsig".into(),
            timestamp: "1609459200".into(),
            nonce: "nonce123".into(),
            echostr: Some("echo".into()),
            openid: None,
        };
        assert!(handle_verification(&config, &query).is_err());
    }

    #[test]
    fn test_handle_verification_missing_echostr() {
        let config = test_config();
        let query = signed_query(&config, "1609459200", "nonce123", None);
        assert!(handle_verification(&config, &query).is_err());
    }

    #[test]
    fn test_parse_callback_text_message() {
        let config = test_config();
        let query = signed_query(&config, "1609459200", "nonce123", None);
        let body = r#"<xml><ToUserName><![CDATA[gh_test]]></ToUserName><FromUserName><![CDATA[oUser]]></FromUserName><CreateTime>1609459200</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[验证码]]></Content></xml>"#;

        let parsed = parse_callback(&config, &query, body).unwrap();
        assert_eq!(parsed.message.msg_type, "text");
        assert_eq!(parsed.message.text_content(), "验证码");
        assert_eq!(parsed.message.from_user_name, "oUser");
    }

    #[test]
    fn test_parse_callback_bad_signature() {
        let config = test_config();
        let query = CallbackQuery {
            signature: "bad".into(),
            timestamp: "1".into(),
            nonce: "n".into(),
            echostr: None,
            openid: None,
        };
        assert!(parse_callback(&config, &query, "<xml/>").is_err());
    }

    #[test]
    fn test_from_params() {
        let q = CallbackQuery::from_params([
            ("signature", "sig"),
            ("timestamp", "ts"),
            ("nonce", "nc"),
            ("echostr", "echo"),
        ]);
        assert_eq!(q.signature, "sig");
        assert_eq!(q.timestamp, "ts");
        assert_eq!(q.nonce, "nc");
        assert_eq!(q.echostr.as_deref(), Some("echo"));
    }
}
