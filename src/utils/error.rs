use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
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
struct ErrorResponse {
    error: String,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
            ApiError::ServiceUnavailable(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable")
            }
        };

        let body = Json(ErrorResponse {
            error: error_type.to_string(),
            message: self.to_string(),
        });

        (status, body).into_response()
    }
}

// Convert anyhow::Error to ApiError
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!("Internal error: {:?}", err);
        ApiError::Internal(err.to_string())
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
        ApiError::Internal(format!("Database error: {}", err))
    }
}

// Convert jsonwebtoken::errors::Error to ApiError
impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        ApiError::Unauthorized(format!("Token error: {}", err))
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
        
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        
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
    fn test_from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("Something went wrong");
        let api_error: ApiError = anyhow_err.into();
        
        match api_error {
            ApiError::Internal(msg) => assert!(msg.contains("Something went wrong")),
            _ => panic!("Expected Internal error"),
        }
    }

    #[test]
    fn test_error_display() {
        let error = ApiError::NotFound("Resource not found".to_string());
        assert_eq!(error.to_string(), "Not found: Resource not found");
        
        let error = ApiError::Forbidden("Access denied".to_string());
        assert_eq!(error.to_string(), "Forbidden: Access denied");
    }
}
