use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::AppState;
use crate::repositories::user::CreateUserInput;
use crate::services::auth::{AuthService, LoginRequest, LoginResponse};
use crate::services::user::UserService;
use crate::services::verification::VerificationService;
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
    Json(payload): Json<LoginRequestBody>,
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
    Json(payload): Json<RegisterRequestBody>,
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
        .create_user(CreateUserInput {
            email: email.clone(),
            password: payload.password,
            name: payload.name,
        })
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
    Json(payload): Json<VerifyEmailRequestBody>,
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
    Json(payload): Json<ResendCodeRequestBody>,
) -> Result<Json<ResendCodeResponse>, ApiError> {
    payload.validate()?;

    // Normalize email to lowercase at the entry point.
    let email = payload.email.to_lowercase();

    let service = VerificationService::new(state.db.clone(), state.email.clone());
    service.resend_code(&email).await?;

    Ok(Json(ResendCodeResponse {
        message: "A new verification code has been sent".to_string(),
    }))
}
