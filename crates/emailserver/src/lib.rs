//! Email service module for webshelf
//!
//! Provides SMTP email sending capabilities:
//! - Registration verification code emails
//! - Password reset emails
//! - Welcome emails
//! - Generic text and HTML email sending
//!
//! # Configuration
//!
//! ```rust
//! use emailserver::EmailConfig;
//!
//! let config = EmailConfig {
//!     smtp_host: "smtp.example.com".to_string(),
//!     smtp_port: 587,
//!     smtp_username: "noreply@example.com".to_string(),
//!     smtp_password: "password".to_string(),
//!     from_address: "noreply@example.com".to_string(),
//!     from_name: Some("Webshelf".to_string()),
//!     use_tls: true,
//!     timeout_secs: 30,
//! };
//! ```
//!
//! # Hot-reload support
//!
//! Configuration can be updated at runtime:
//! ```rust,ignore
//! email_service.update_config(new_config).await;
//! ```

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// SMTP email configuration
#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    /// SMTP server hostname
    #[serde(default)]
    pub smtp_host: String,

    /// SMTP server port (465 for implicit TLS, 587 for STARTTLS, 25 for plain)
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    /// SMTP authentication username
    #[serde(default)]
    pub smtp_username: String,

    /// SMTP authentication password
    #[serde(default)]
    pub smtp_password: String,

    /// From email address
    #[serde(default)]
    pub from_address: String,

    /// Optional display name for the from address
    #[serde(default)]
    pub from_name: Option<String>,

    /// Use TLS (STARTTLS or implicit TLS depending on port)
    #[serde(default = "default_use_tls")]
    pub use_tls: bool,

    /// Connection timeout in seconds (0 = no timeout)
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Serialize for EmailConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("EmailConfig", 8)?;
        state.serialize_field("smtp_host", &self.smtp_host)?;
        state.serialize_field("smtp_port", &self.smtp_port)?;
        state.serialize_field("smtp_username", &self.smtp_username)?;
        state.serialize_field("smtp_password", "[REDACTED]")?;
        state.serialize_field("from_address", &self.from_address)?;
        state.serialize_field("from_name", &self.from_name)?;
        state.serialize_field("use_tls", &self.use_tls)?;
        state.serialize_field("timeout_secs", &self.timeout_secs)?;
        state.end()
    }
}

fn default_smtp_port() -> u16 {
    587
}

fn default_use_tls() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    30
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: String::new(),
            smtp_port: default_smtp_port(),
            smtp_username: String::new(),
            smtp_password: String::new(),
            from_address: String::new(),
            from_name: None,
            use_tls: default_use_tls(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

impl EmailConfig {
    /// Check if the configuration is complete for sending emails
    pub fn is_configured(&self) -> bool {
        !self.smtp_host.is_empty()
            && !self.smtp_username.is_empty()
            && !self.smtp_password.is_empty()
            && !self.from_address.is_empty()
    }
}

/// Email sending error
#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    /// Email service is not configured
    #[error("Email service not configured")]
    NotConfigured,

    /// Invalid configuration
    #[error("Invalid email config: {0}")]
    InvalidConfig(String),

    /// Invalid email address
    #[error("Invalid email address: {0}")]
    InvalidAddress(String),

    /// Email build error
    #[error("Failed to build email: {0}")]
    BuildError(String),

    /// SMTP send error
    #[error("Failed to send email: {0}")]
    SendError(String),
}

#[derive(Clone)]
enum TransportState {
    Disabled,
    InvalidConfig(String),
    Ready(AsyncSmtpTransport<Tokio1Executor>),
}

impl TransportState {
    fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SmtpSecurityMode {
    StartTls,
    ImplicitTls,
    Plain,
}

#[derive(Clone)]
struct EmailRuntime {
    config: EmailConfig,
    transport: TransportState,
}

/// Email service for sending emails via SMTP
#[derive(Clone)]
pub struct EmailService {
    runtime: Arc<RwLock<EmailRuntime>>,
}

fn smtp_timeout(timeout_secs: u64) -> Option<Duration> {
    if timeout_secs == 0 {
        None
    } else {
        Some(Duration::from_secs(timeout_secs))
    }
}

fn smtp_security_mode(config: &EmailConfig) -> SmtpSecurityMode {
    if config.use_tls {
        if config.smtp_port == 465 {
            SmtpSecurityMode::ImplicitTls
        } else {
            SmtpSecurityMode::StartTls
        }
    } else {
        SmtpSecurityMode::Plain
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn build_welcome_greeting(name: Option<&str>) -> (String, String) {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => (
            format!("Hello, {}!", name),
            format!("Hello, {}!", escape_html(name)),
        ),
        None => ("Hello!".to_string(), "Hello!".to_string()),
    }
}

impl EmailService {
    /// Create a new email service instance
    pub fn new(config: EmailConfig) -> Self {
        let transport = Self::build_transport(&config);
        Self {
            runtime: Arc::new(RwLock::new(EmailRuntime { config, transport })),
        }
    }

    /// Create from Arc<EmailConfig>.
    ///
    /// Note: the inner EmailConfig is cloned; the Arc is not shared.
    pub fn from_shared(config: Arc<EmailConfig>) -> Self {
        Self::new((*config).clone())
    }

    /// Build the SMTP transport
    fn build_transport(config: &EmailConfig) -> TransportState {
        if !config.is_configured() {
            tracing::warn!("Email service not configured, email sending will be disabled");
            return TransportState::Disabled;
        }

        let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());
        let timeout = smtp_timeout(config.timeout_secs);

        tracing::info!(
            host = %config.smtp_host,
            port = config.smtp_port,
            use_tls = config.use_tls,
            "Building SMTP transport"
        );

        let transport = match smtp_security_mode(config) {
            SmtpSecurityMode::StartTls => {
                match AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host) {
                    Ok(builder) => builder
                        .credentials(creds)
                        .authentication(vec![
                            lettre::transport::smtp::authentication::Mechanism::Plain,
                            lettre::transport::smtp::authentication::Mechanism::Login,
                        ])
                        .port(config.smtp_port)
                        .timeout(timeout)
                        .build(),
                    Err(e) => {
                        let msg = format!(
                            "Cannot build STARTTLS connection for SMTP host '{}': {}",
                            config.smtp_host, e
                        );
                        tracing::error!(error = %msg, "Email service configuration error");
                        return TransportState::InvalidConfig(msg);
                    }
                }
            }
            SmtpSecurityMode::ImplicitTls => {
                match AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host) {
                    Ok(builder) => builder
                        .credentials(creds)
                        .authentication(vec![
                            lettre::transport::smtp::authentication::Mechanism::Plain,
                            lettre::transport::smtp::authentication::Mechanism::Login,
                        ])
                        .port(config.smtp_port)
                        .timeout(timeout)
                        .build(),
                    Err(e) => {
                        let msg = format!(
                            "Cannot build SMTPS connection for SMTP host '{}': {}",
                            config.smtp_host, e
                        );
                        tracing::error!(error = %msg, "Email service configuration error");
                        return TransportState::InvalidConfig(msg);
                    }
                }
            }
            SmtpSecurityMode::Plain => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
                    .credentials(creds)
                    .port(config.smtp_port)
                    .timeout(timeout)
                    .build()
            }
        };

        TransportState::Ready(transport)
    }

    /// Check if the service is configured
    pub async fn is_configured(&self) -> bool {
        self.runtime.read().await.transport.is_ready()
    }

    /// Update configuration at runtime
    pub async fn update_config(&self, config: EmailConfig) {
        let transport = Self::build_transport(&config);
        let mut runtime = self.runtime.write().await;
        *runtime = EmailRuntime { config, transport };
        tracing::info!("Email service configuration updated");
    }

    /// Get a clone of the current configuration
    pub async fn config(&self) -> EmailConfig {
        self.runtime.read().await.config.clone()
    }

    /// Send a registration verification code email
    pub async fn send_registration_code_email(
        &self,
        to: &str,
        code: &str,
        expires_minutes: i64,
    ) -> Result<(), EmailError> {
        let subject = "Your Registration Code";
        let text_body = format!(
            r#"Hello!

You are registering for Webshelf.

Your email verification code is: {}

The code will expire in {} minutes. If you did not request this, please ignore this email.

Best regards,
Webshelf Team
"#,
            code, expires_minutes
        );

        let html_body = format!(
            r#"<html>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<h2 style="color: #2c5282;">Your Registration Code</h2>
<p>Hello!</p>
<p>You are registering for Webshelf.</p>
<p>Please enter the following code to complete your registration:</p>
<div style="margin: 24px 0; padding: 16px; background: #f7fafc; border: 1px solid #e2e8f0; border-radius: 8px; text-align: center;">
<span style="font-size: 28px; letter-spacing: 8px; font-weight: bold; color: #2d3748;">{}</span>
</div>
<p style="color: #718096; font-size: 14px;">The code will expire in {} minutes. If you did not request this, please ignore this email.</p>
<hr style="border: none; border-top: 1px solid #e2e8f0; margin: 20px 0;">
<p style="color: #718096; font-size: 12px;">Webshelf Team</p>
</div>
</body>
</html>"#,
            code, expires_minutes
        );

        self.send_html_email(to, subject, &text_body, &html_body)
            .await
    }

    /// Send a password-reset verification code email
    /// this sends a short numeric code that the user manually types into the
    /// reset-password form. This avoids the need for a frontend route that
    /// accepts a token in the URL path.
    pub async fn send_password_reset_code_email(
        &self,
        to: &str,
        code: &str,
        expires_minutes: i64,
    ) -> Result<(), EmailError> {
        let subject = "Your Password Reset Code";
        let text_body = format!(
            r#"Hello!

We received a request to reset your password.

Your password reset code is: {}

This code will expire in {} minutes. If you did not request a password reset, please ignore this email.

Best regards,
Webshelf Team
"#,
            code, expires_minutes
        );

        let html_body = format!(
            r#"<html>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<h2 style="color: #2c5282;">Password Reset Code</h2>
<p>Hello!</p>
<p>We received a request to reset your password.</p>
<p>Enter the following code to reset your password:</p>
<div style="margin: 24px 0; padding: 16px; background: #f7fafc; border: 1px solid #e2e8f0; border-radius: 8px; text-align: center;">
<span style="font-size: 28px; letter-spacing: 8px; font-weight: bold; color: #2d3748;">{}</span>
</div>
<p style="color: #718096; font-size: 14px;">This code will expire in {} minutes. If you did not request a password reset, please ignore this email.</p>
<hr style="border: none; border-top: 1px solid #e2e8f0; margin: 20px 0;">
<p style="color: #718096; font-size: 12px;">Webshelf Team</p>
</div>
</body>
</html>"#,
            code, expires_minutes
        );

        self.send_html_email(to, subject, &text_body, &html_body)
            .await
    }

    /// Send a welcome email after email verification
    pub async fn send_welcome_email(&self, to: &str, name: Option<&str>) -> Result<(), EmailError> {
        let (text_greeting, html_greeting) = build_welcome_greeting(name);

        let subject = "Welcome to Webshelf";
        let text_body = format!(
            "{text_greeting}\n\nCongratulations on verifying your email address.\n\nYou can now start using all features of Webshelf.\n\nIf you have any questions, please contact our support team.\n\nBest regards,\nWebshelf Team\n"
        );

        let html_body = format!(
            r#"<html>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<h2 style="color: #2c5282;">Welcome to Webshelf</h2>
<p>{}</p>
<p>Congratulations on verifying your email address.</p>
<p>You can now start using all features of Webshelf.</p>
<p>If you have any questions, please contact our support team.</p>
<hr style="border: none; border-top: 1px solid #e2e8f0; margin: 20px 0;">
<p style="color: #718096; font-size: 12px;">Webshelf Team</p>
</div>
</body>
</html>"#,
            html_greeting
        );

        self.send_html_email(to, subject, &text_body, &html_body)
            .await
    }

    /// Send a plain text email
    pub async fn send_text_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), EmailError> {
        let runtime = self.runtime.read().await;

        let transport = match &runtime.transport {
            TransportState::Ready(transport) => transport,
            TransportState::Disabled => return Err(EmailError::NotConfigured),
            TransportState::InvalidConfig(msg) => {
                return Err(EmailError::InvalidConfig(msg.clone()));
            }
        };

        let from_mailbox = Self::build_from_mailbox(&runtime.config)?;

        let to_mailbox: Mailbox = to
            .parse()
            .map_err(|_| EmailError::InvalidAddress(to.to_string()))?;

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .body(body.to_string())
            .map_err(|e| EmailError::BuildError(e.to_string()))?;

        transport
            .send(email)
            .await
            .map_err(|e| EmailError::SendError(e.to_string()))?;

        tracing::info!(to = %to, subject = %subject, "Email sent successfully");
        Ok(())
    }

    /// Build the from mailbox
    fn build_from_mailbox(config: &EmailConfig) -> Result<Mailbox, EmailError> {
        let from_str = match &config.from_name {
            Some(name) => format!("{} <{}>", name, config.from_address),
            None => config.from_address.clone(),
        };

        from_str.parse().map_err(|_| {
            EmailError::BuildError(format!("Invalid from address: {}", config.from_address))
        })
    }

    /// Send a multipart email with both plain text and HTML body
    pub async fn send_html_email(
        &self,
        to: &str,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<(), EmailError> {
        let runtime = self.runtime.read().await;

        let transport = match &runtime.transport {
            TransportState::Ready(transport) => transport,
            TransportState::Disabled => return Err(EmailError::NotConfigured),
            TransportState::InvalidConfig(msg) => {
                return Err(EmailError::InvalidConfig(msg.clone()));
            }
        };

        let from_mailbox = Self::build_from_mailbox(&runtime.config)?;

        let to_mailbox: Mailbox = to
            .parse()
            .map_err(|_| EmailError::InvalidAddress(to.to_string()))?;

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .multipart(
                lettre::message::MultiPart::alternative()
                    .singlepart(
                        lettre::message::SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text_body.to_string()),
                    )
                    .singlepart(
                        lettre::message::SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html_body.to_string()),
                    ),
            )
            .map_err(|e| EmailError::BuildError(e.to_string()))?;

        transport
            .send(email)
            .await
            .map_err(|e| EmailError::SendError(e.to_string()))?;

        tracing::info!(to = %to, subject = %subject, "HTML email sent successfully");
        Ok(())
    }
}

impl std::fmt::Debug for EmailService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Omit async lock access — just show the type name.
        f.debug_struct("EmailService").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> EmailConfig {
        EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 465,
            smtp_username: "test@example.com".to_string(),
            smtp_password: "testpass".to_string(),
            from_address: "noreply@example.com".to_string(),
            from_name: Some("Webshelf".to_string()),
            use_tls: true,
            timeout_secs: 30,
        }
    }

    #[tokio::test]
    async fn test_email_service_creation() {
        let service = EmailService::new(test_config());
        assert!(service.is_configured().await);
    }

    #[tokio::test]
    async fn test_localhost_email_service_creation() {
        let mut config = test_config();
        config.smtp_host = "localhost".to_string();
        config.use_tls = false;

        let service = EmailService::new(config);
        assert!(service.is_configured().await);
    }

    #[tokio::test]
    async fn test_email_service_not_configured() {
        let service = EmailService::new(EmailConfig::default());
        assert!(!service.is_configured().await);
    }

    #[tokio::test]
    async fn test_invalid_email_address() {
        let service = EmailService::new(test_config());
        let result = service
            .send_text_email("invalid-email", "Test", "Body")
            .await;
        assert!(matches!(result, Err(EmailError::InvalidAddress(_))));
    }

    #[tokio::test]
    async fn test_send_without_config() {
        let service = EmailService::new(EmailConfig::default());
        let result = service
            .send_text_email("test@example.com", "Test", "Body")
            .await;
        assert!(matches!(result, Err(EmailError::NotConfigured)));
    }

    #[tokio::test]
    async fn test_config_update() {
        let service = EmailService::new(EmailConfig::default());
        assert!(!service.is_configured().await);

        let new_config = test_config();
        service.update_config(new_config).await;

        assert!(service.is_configured().await);
    }

    #[tokio::test]
    async fn test_from_shared_usage() {
        let mut config = test_config();
        config.from_name = Some("Test Sender".to_string());

        let service = EmailService::from_shared(Arc::new(config));
        let cfg = service.config().await;

        assert_eq!(cfg.from_name, Some("Test Sender".to_string()));
    }

    #[test]
    fn test_smtp_timeout_zero_disables_timeout() {
        assert_eq!(smtp_timeout(0), None);
        assert_eq!(smtp_timeout(30), Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_smtp_security_mode_starttls() {
        let mut config = test_config();
        config.smtp_port = 587;
        config.use_tls = true;
        assert_eq!(smtp_security_mode(&config), SmtpSecurityMode::StartTls);
    }

    #[test]
    fn test_smtp_security_mode_implicit_tls() {
        let mut config = test_config();
        config.smtp_port = 465;
        config.use_tls = true;
        assert_eq!(smtp_security_mode(&config), SmtpSecurityMode::ImplicitTls);
    }

    #[test]
    fn test_smtp_security_mode_plain() {
        let mut config = test_config();
        config.use_tls = false;
        assert_eq!(smtp_security_mode(&config), SmtpSecurityMode::Plain);
    }

    #[test]
    fn test_build_welcome_greeting_without_name() {
        let (text_greeting, html_greeting) = build_welcome_greeting(None);
        assert_eq!(text_greeting, "Hello!");
        assert_eq!(html_greeting, "Hello!");
    }

    #[test]
    fn test_build_welcome_greeting_trims_blank_name() {
        let (text_greeting, html_greeting) = build_welcome_greeting(Some("   "));
        assert_eq!(text_greeting, "Hello!");
        assert_eq!(html_greeting, "Hello!");
    }

    #[test]
    fn test_build_welcome_greeting_escapes_html_name() {
        let (text_greeting, html_greeting) = build_welcome_greeting(Some(" <b>Alice & Bob</b> "));
        assert_eq!(text_greeting, "Hello, <b>Alice & Bob</b>!");
        assert_eq!(html_greeting, "Hello, &lt;b&gt;Alice &amp; Bob&lt;/b&gt;!");
    }

    #[test]
    fn test_email_config_is_configured() {
        let config = EmailConfig::default();
        assert!(!config.is_configured());

        let config = test_config();
        assert!(config.is_configured());
    }

    #[test]
    fn test_email_config_serialization_redacts_password() {
        let config = test_config();
        assert_eq!(config.smtp_password, "testpass");

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("[REDACTED]"));
        assert!(!json.contains("testpass"));
        assert!(json.contains("smtp.example.com"));
        assert!(json.contains("noreply@example.com"));
    }
}
