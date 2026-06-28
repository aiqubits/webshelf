//! Unified error type for the WeChat SDK.

use std::fmt;

/// All errors produced by the `wechat-api` crate.
#[derive(Debug, thiserror::Error)]
pub enum WechatError {
    /// Missing or incomplete WeChat configuration.
    #[error("WeChat configuration incomplete: {0}")]
    ConfigIncomplete(String),

    /// Signature verification failed on an incoming WeChat callback.
    #[error("WeChat signature verification failed")]
    SignatureMismatch,

    /// Failed to parse the incoming XML message body.
    #[error("Failed to parse WeChat XML message: {0}")]
    XmlParse(String),

    /// Message was flagged as encrypted but the AES key / decryption failed.
    #[error("Failed to decrypt WeChat message: {0}")]
    Decrypt(String),

    /// The captcha code was not found or has expired.
    #[error("Captcha not found or expired")]
    CaptchaNotFound,

    /// The captcha code does not match the stored value.
    #[error("Captcha mismatch")]
    CaptchaMismatch,

    /// The user has exceeded the maximum number of failed attempts.
    #[error("Too many failed captcha attempts")]
    TooManyAttempts,

    /// A captcha request arrived within the resend cooldown window.
    #[error("Captcha resend cooldown active")]
    CooldownActive,

    /// No user is bound to the given WeChat openid.
    #[error("No user bound to openid: {0}")]
    UserNotBound(String),

    /// An HTTP request to a WeChat Platform API failed.
    #[error("WeChat API request failed: {0}")]
    ApiRequest(String),

    /// The WeChat Platform API returned a business-level error
    /// (non-zero `errcode`).
    #[error("WeChat API error (code={errcode}): {errmsg}")]
    ApiBusiness { errcode: i64, errmsg: String },

    /// A backing store (cache / database) operation failed.
    #[error("Store error: {0}")]
    Store(String),

    /// Catch-all for unexpected internal failures.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Convenience `Result` alias used throughout the crate.
pub type WechatResult<T> = std::result::Result<T, WechatError>;

impl WechatError {
    /// Wrap any error display string into a [`WechatError::Store`].
    pub fn store<E: fmt::Display>(e: E) -> Self {
        WechatError::Store(e.to_string())
    }
}
