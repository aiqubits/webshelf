use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

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

#[derive(Serialize)]
pub(crate) struct ErrorResponse {
    error: String,
    message: String,
}

impl ErrorResponse {
    pub(crate) fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_error"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
            ApiError::ServiceUnavailable(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable")
            }
        };

        let body = Json(ErrorResponse::new(error_type, self.to_string()));

        (status, body).into_response()
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

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn extract_error_json(response: Response) -> serde_json::Value {
        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_bad_request_response() {
        let error = ApiError::BadRequest("Invalid input".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let json = extract_error_json(response).await;
        assert_eq!(json["error"], "bad_request");
        assert!(json["message"].as_str().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn test_unauthorized_response() {
        let error = ApiError::Unauthorized("Token expired".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let json = extract_error_json(response).await;
        assert_eq!(json["error"], "unauthorized");
        assert!(json["message"].as_str().unwrap().contains("Token expired"));
    }

    #[tokio::test]
    async fn test_not_found_response() {
        let error = ApiError::NotFound("User not found".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let json = extract_error_json(response).await;
        assert_eq!(json["error"], "not_found");
    }

    #[tokio::test]
    async fn test_validation_response() {
        let error = ApiError::Validation("Email is invalid".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let json = extract_error_json(response).await;
        assert_eq!(json["error"], "validation_error");
    }

    #[tokio::test]
    async fn test_internal_error_response() {
        let error = ApiError::Internal("Database connection failed".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = extract_error_json(response).await;
        assert_eq!(json["error"], "internal_error");
    }

    #[test]
    fn test_error_display() {
        let error = ApiError::NotFound("Resource not found".to_string());
        assert_eq!(error.to_string(), "Not found: Resource not found");

        let error = ApiError::Forbidden("Access denied".to_string());
        assert_eq!(error.to_string(), "Forbidden: Access denied");
    }
}
