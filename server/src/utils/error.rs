use thiserror::Error;
use webshelf_runtime::HttpError;

/// Unified API error type for HTTP boundary
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

// Convert ApiError to HttpError for unified handler support
impl From<ApiError> for HttpError {
    fn from(err: ApiError) -> Self {
        match err {
            ApiError::BadRequest(msg) => HttpError::bad_request(msg),
            ApiError::Unauthorized(msg) => HttpError::unauthorized(msg),
            ApiError::Forbidden(msg) => HttpError::forbidden(msg),
            ApiError::NotFound(msg) => HttpError::not_found(msg),
            ApiError::Conflict(msg) => HttpError::conflict(msg),
            ApiError::Validation(msg) => {
                // Validation errors use a specific error_type
                let mut http_err = HttpError::bad_request(msg);
                http_err.error_type = "validation_error";
                http_err
            }
            ApiError::Internal(_) => HttpError::internal("An unexpected error occurred"),
            ApiError::ServiceUnavailable(msg) => HttpError::service_unavailable(msg),
        }
    }
}

// Convert validator::ValidationErrors to ApiError
impl From<validator::ValidationErrors> for ApiError {
    fn from(err: validator::ValidationErrors) -> Self {
        ApiError::Validation(err.to_string())
    }
}

// Convert sea_orm::DbErr to ApiError
impl From<sea_orm::DbErr> for ApiError {
    fn from(err: sea_orm::DbErr) -> Self {
        tracing::error!("Database error: {:?}", err);
        ApiError::Internal("An unexpected database error occurred".to_string())
    }
}

// Convert jsonwebtoken::errors::Error to ApiError
// NOTE: Internal error details are intentionally not exposed to the client
// to prevent attackers from inferring token structure or validity.
impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        tracing::warn!("JWT token error: {}", err);
        ApiError::Unauthorized("Invalid or expired token".to_string())
    }
}

// Convert UserError to ApiError for type-safe error mapping
impl From<crate::services::user::UserError> for ApiError {
    fn from(err: crate::services::user::UserError) -> Self {
        match err {
            crate::services::user::UserError::NotFound => {
                ApiError::NotFound("User not found".to_string())
            }
            crate::services::user::UserError::EmailConflict => {
                ApiError::Conflict("Email already registered".to_string())
            }
            crate::services::user::UserError::InvalidCredentials => {
                ApiError::Unauthorized("Current password is incorrect".to_string())
            }
            crate::services::user::UserError::Forbidden(msg) => ApiError::Forbidden(msg),
            crate::services::user::UserError::WeakPassword(msg) => ApiError::BadRequest(msg),
            crate::services::user::UserError::SamePassword(msg) => ApiError::BadRequest(msg),
            crate::services::user::UserError::NotAllowed(msg) => ApiError::Forbidden(msg),
            crate::services::user::UserError::Internal(e) => {
                tracing::error!("Internal error: {:?}", e);
                ApiError::Internal("An unexpected error occurred".to_string())
            }
        }
    }
}

// Convert AuthError to ApiError for type-safe error mapping
impl From<crate::services::auth::AuthError> for ApiError {
    fn from(err: crate::services::auth::AuthError) -> Self {
        match err {
            crate::services::auth::AuthError::InvalidCredentials => {
                ApiError::Unauthorized("Invalid email or password".to_string())
            }
            crate::services::auth::AuthError::Internal(e) => {
                tracing::error!("Auth internal error: {:?}", e);
                ApiError::Internal("An unexpected error occurred".to_string())
            }
        }
    }
}

// Convert VerificationError to ApiError for type-safe error mapping
impl From<crate::services::verification::VerificationError> for ApiError {
    fn from(err: crate::services::verification::VerificationError) -> Self {
        match err {
            crate::services::verification::VerificationError::InvalidOrExpired => {
                ApiError::BadRequest("Invalid or expired verification code".to_string())
            }
            crate::services::verification::VerificationError::TooManyAttempts => {
                // Mapped to 400 (not 403) to prevent user enumeration:
                // an attacker should not be able to distinguish "user does
                // not exist" from "user exists but is locked out".
                tracing::warn!("User exceeded max verification attempts");
                ApiError::BadRequest("Invalid or expired verification code".to_string())
            }
            crate::services::verification::VerificationError::TooSoon => {
                ApiError::BadRequest("Please wait before requesting a new code".to_string())
            }
            crate::services::verification::VerificationError::EmailNotConfigured => {
                tracing::warn!("Email service not configured for verification");
                ApiError::ServiceUnavailable(
                    "Email verification is currently unavailable".to_string(),
                )
            }
            crate::services::verification::VerificationError::Internal(e) => {
                tracing::error!("Verification internal error: {:?}", e);
                ApiError::Internal("An unexpected error occurred".to_string())
            }
        }
    }
}

// Convert PasswordResetError to ApiError for type-safe error mapping.
//
// All "code-side" failures (invalid/expired, too-soon, too-many-attempts)
// are mapped to 400 BadRequest with a generic message so that an attacker
// cannot distinguish "user does not exist" from "code is wrong/expired"
// or "user is locked out". This matches the anti-enumeration posture of
// the verification flow.
impl From<crate::services::password_reset::PasswordResetError> for ApiError {
    fn from(err: crate::services::password_reset::PasswordResetError) -> Self {
        match err {
            crate::services::password_reset::PasswordResetError::InvalidOrExpired => {
                ApiError::BadRequest("Invalid or expired reset code".to_string())
            }
            crate::services::password_reset::PasswordResetError::TooManyAttempts => {
                tracing::warn!("User exceeded max password-reset attempts");
                ApiError::BadRequest("Invalid or expired reset code".to_string())
            }
            crate::services::password_reset::PasswordResetError::TooSoon => {
                ApiError::BadRequest("Please wait before requesting a new reset code".to_string())
            }
            crate::services::password_reset::PasswordResetError::EmailNotConfigured => {
                tracing::warn!("Email service not configured for password reset");
                ApiError::ServiceUnavailable("Password reset is currently unavailable".to_string())
            }
            crate::services::password_reset::PasswordResetError::Internal(e) => {
                tracing::error!("Password-reset internal error: {:?}", e);
                ApiError::Internal("An unexpected error occurred".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = ApiError::NotFound("Resource not found".to_string());
        assert_eq!(error.to_string(), "Not found: Resource not found");

        let error = ApiError::Forbidden("Access denied".to_string());
        assert_eq!(error.to_string(), "Forbidden: Access denied");
    }
}
