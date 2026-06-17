use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::AppState;
use crate::repositories::user::CreateUserInput;
use crate::services::auth::{AuthService, LoginRequest, LoginResponse};
use crate::services::password_reset::{PasswordResetError, PasswordResetService};
use crate::services::user::UserService;
use crate::services::verification::{VerificationError, VerificationService};
use crate::utils::JsonOrForm;
use crate::utils::error::ApiError;
use crate::utils::validator::check_password_strength;

/// Login request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 1, message = "password is required"))]
    password: String,
}

/// Login endpoint
pub async fn login(
    State(state): State<AppState>,
    JsonOrForm(payload): JsonOrForm<LoginRequestBody>,
) -> Result<Json<LoginResponse>, ApiError> {
    payload.validate()?;

    let service = AuthService::new(
        state.db.clone(),
        state.config.jwt_secret.clone(),
        state.config.jwt_expiry_seconds,
    );

    let result = service
        .login(LoginRequest {
            email: payload.email.to_lowercase(),
            password: payload.password,
        })
        .await?;

    Ok(Json(result))
}

/// Register request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    password: String,

    #[validate(length(
        min = 2,
        max = 50,
        message = "name must be between 2 and 50 characters"
    ))]
    name: String,
}

/// Register response
#[derive(Serialize)]
pub struct RegisterResponse {
    message: String,
    user_id: String,
    /// Whether the email is already verified.
    /// When the email service is not configured, registration auto-verifies.
    email_verified: bool,
}

/// Register endpoint
pub async fn register(
    State(state): State<AppState>,
    JsonOrForm(payload): JsonOrForm<RegisterRequestBody>,
) -> Result<Json<RegisterResponse>, ApiError> {
    payload.validate().inspect_err(|e| {
        tracing::warn!("Registration validation failed: {:?}", e);
    })?;

    check_password_strength(&payload.password)?;

    // Normalize email to lowercase once at the entry point to avoid
    // redundant normalization in multiple downstream call sites.
    let email = payload.email.to_lowercase();

    let service = UserService::new(state.db.clone());
    let user = service
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: payload.password,
                name: payload.name,
                role: None,
            },
            "user",
        )
        .await?;

    let verification = VerificationService::new(state.db.clone(), state.email.clone());
    let (message, email_verified) = match verification.send_verification_email(&email).await {
        Ok(()) => ("Verification code sent to your email".to_string(), false),
        Err(crate::services::verification::VerificationError::EmailNotConfigured) => {
            tracing::warn!("Email service not configured — auto-verifying user");
            if let Err(e) = verification.auto_verify(&email).await {
                tracing::error!("Failed to auto-verify user: {:?}", e);
                return Err(ApiError::Internal(
                    "Registration failed due to an internal error. Please try again later."
                        .to_string(),
                ));
            }
            ("User registered successfully".to_string(), true)
        }
        Err(e) => {
            // When SMTP is configured but the send fails (transient network error,
            // etc.), auto-verify the user as a fallback instead of deleting the
            // account.  This avoids orphan accounts that can neither log in
            // (email_verified=false) nor re-register (email already taken), and
            // eliminates the crash window between insert and manual DELETE.
            tracing::error!("Failed to send verification email: {:?}", e);
            if let Err(verify_err) = verification.auto_verify(&email).await {
                tracing::error!(
                    "Failed to auto-verify after email failure: {:?}",
                    verify_err
                );
                return Err(ApiError::Internal(
                    "Registration failed due to an internal error. Please try again later."
                        .to_string(),
                ));
            }
            ("User registered successfully. Note: verification email could not be sent, but your account is active.".to_string(), true)
        }
    };

    tracing::trace!("User registered successfully with id: {}", user.id);
    Ok(Json(RegisterResponse {
        message,
        user_id: user.id.to_string(),
        email_verified,
    }))
}

/// Verify email request
#[derive(Debug, Deserialize, Validate)]
pub struct VerifyEmailRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 6, max = 6, message = "code must be 6 characters"))]
    code: String,
}

/// Verify email response
#[derive(Serialize)]
pub struct VerifyEmailResponse {
    message: String,
}

/// Verify email endpoint
pub async fn verify_email(
    State(state): State<AppState>,
    JsonOrForm(payload): JsonOrForm<VerifyEmailRequestBody>,
) -> Result<Json<VerifyEmailResponse>, ApiError> {
    payload.validate()?;

    // Reject non-numeric codes early to avoid wasting Argon2 CPU
    // on obviously invalid inputs.
    if !payload.code.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::BadRequest("code must be 6 digits".to_string()));
    }

    // Normalize email to lowercase at the entry point.
    let email = payload.email.to_lowercase();

    let service = VerificationService::new(state.db.clone(), state.email.clone());
    service.verify_email(&email, &payload.code).await?;

    Ok(Json(VerifyEmailResponse {
        message: "Email verified successfully".to_string(),
    }))
}

/// Resend verification code request
#[derive(Debug, Deserialize, Validate)]
pub struct ResendCodeRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,
}

/// Resend code response
#[derive(Serialize)]
pub struct ResendCodeResponse {
    message: String,
}

/// Resend verification code endpoint
pub async fn resend_code(
    State(state): State<AppState>,
    JsonOrForm(payload): JsonOrForm<ResendCodeRequestBody>,
) -> Result<Json<ResendCodeResponse>, ApiError> {
    payload.validate()?;

    // Normalize email to lowercase at the entry point.
    let email = payload.email.to_lowercase();

    let service = VerificationService::new(state.db.clone(), state.email.clone());

    // Anti-enumeration: swallow `TooSoon` to prevent leaking whether an
    // email is registered via the cooldown window.  Non-existent users
    // always get 200; existent users within cooldown must also get 200
    // (the same generic message) so that an attacker cannot distinguish
    // the two cases by measuring response differences between first and
    // second request.
    if let Err(e) = service.resend_code(&email).await {
        if matches!(e, VerificationError::TooSoon) {
            tracing::info!(
                "Resend-code within cooldown for {} (TooSoon swallowed)",
                email
            );
        } else {
            return Err(e.into());
        }
    }

    Ok(Json(ResendCodeResponse {
        message: "If that email is registered, a new verification code has been sent".to_string(),
    }))
}

/// Forgot-password request — initiate a password-reset email.
///
/// Always returns 200 OK on a syntactically valid email to prevent
/// user enumeration; the response body is identical regardless of whether
/// the email is registered. Cooldown errors are swallowed (still 200) so
/// that attackers cannot distinguish registered from unregistered emails
/// by sending a second request within the cooldown window. SMTP
/// configuration failures surface as 503 only for registered emails.
#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,
}

#[derive(Serialize)]
pub struct ForgotPasswordResponse {
    message: String,
}

pub async fn forgot_password(
    State(state): State<AppState>,
    JsonOrForm(payload): JsonOrForm<ForgotPasswordRequestBody>,
) -> Result<Json<ForgotPasswordResponse>, ApiError> {
    payload.validate()?;

    let email = payload.email.to_lowercase();

    let service = PasswordResetService::new(state.db.clone(), state.email.clone());

    // Anti-enumeration: swallow `TooSoon` to prevent leaking whether an
    // email is registered via the cooldown window. Non-existent users
    // always get 200; existent users within cooldown must also get 200
    // (the same generic message) so that an attacker cannot distinguish
    // the two cases by measuring response differences between first and
    // second request.
    if let Err(e) = service.request_reset(&email).await {
        if matches!(e, PasswordResetError::TooSoon) {
            tracing::info!(
                "Forgot-password within cooldown for {} (TooSoon swallowed)",
                email
            );
        } else {
            return Err(e.into());
        }
    }

    // The message intentionally does not reveal whether the email is registered.
    Ok(Json(ForgotPasswordResponse {
        message: "If that email is registered, a reset code has been sent".to_string(),
    }))
}

/// Reset-password request — consume the verification code sent in the
/// reset email and replace the user's password.
///
/// On success, returns a fresh JWT so the user is auto-logged-in.
#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 6, max = 6, message = "code must be 6 digits"))]
    code: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    new_password: String,
}

#[derive(Serialize)]
pub struct ResetPasswordResponse {
    message: String,
    /// Fresh JWT issued after the password is replaced.
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
}

pub async fn reset_password(
    State(state): State<AppState>,
    JsonOrForm(payload): JsonOrForm<ResetPasswordRequestBody>,
) -> Result<Json<ResetPasswordResponse>, ApiError> {
    payload.validate()?;
    check_password_strength(&payload.new_password)?;

    // Reject non-numeric codes early to avoid wasting Argon2 CPU
    // on obviously invalid inputs.
    if !payload.code.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::BadRequest("code must be 6 digits".to_string()));
    }

    let email = payload.email.to_lowercase();

    let service = PasswordResetService::new(state.db.clone(), state.email.clone());
    let outcome = service
        .reset_password(&email, &payload.code, &payload.new_password)
        .await?;

    let new_token = crate::middlewares::auth::generate_token(
        &outcome.user_id.to_string(),
        &outcome.role,
        &state.config.jwt_secret,
        state.config.jwt_expiry_seconds,
        outcome.token_version,
    )
    .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;

    tracing::info!("Password reset completed for user {}", outcome.user_id);
    Ok(Json(ResetPasswordResponse {
        message: "Password reset successfully".to_string(),
        token: new_token,
        token_type: "Bearer".to_string(),
        expires_in: state.config.jwt_expiry_seconds,
        user_id: outcome.user_id.to_string(),
        role: outcome.role,
    }))
}
