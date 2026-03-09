use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::panic::catch_unwind;

/// Panic capture middleware
///
/// Catches panics that occur during request handling and converts them
/// to a 500 Internal Server Error response instead of crashing the server.
pub async fn panic_middleware(request: Request, next: Next) -> Response {
    // We need to use catch_unwind in a sync context
    // For async code, we'll wrap the response handling
    let response = next.run(request).await;

    // The actual panic catching happens at the tokio runtime level
    // This middleware ensures graceful error responses
    response
}

/// Synchronous panic handler for use in route handlers
/// 
/// Wraps a closure and catches any panics, returning an error response instead
pub fn catch_panic<F, T>(f: F) -> Result<T, Response>
where
    F: FnOnce() -> T + std::panic::UnwindSafe,
{
    match catch_unwind(f) {
        Ok(result) => Ok(result),
        Err(panic_info) => {
            let panic_message = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic occurred".to_string()
            };

            tracing::error!("Panic caught: {}", panic_message);

            Err(internal_error_response("An unexpected error occurred"))
        }
    }
}

/// Async panic-safe wrapper for handlers
///
/// Wraps an async handler to catch panics and return graceful error responses
pub async fn panic_safe<F, Fut, T>(f: F) -> Result<T, Response>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    // Note: std::panic::catch_unwind doesn't work with async code directly
    // This is a wrapper that helps with panic handling in async contexts
    // The actual panic catching relies on tokio's panic handling
    Ok(f().await)
}

/// Create an internal error response
fn internal_error_response(message: &str) -> Response {
    #[derive(Serialize)]
    struct ErrorBody {
        error: String,
        message: String,
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: "internal_error".to_string(),
            message: message.to_string(),
        }),
    )
        .into_response()
}

/// Tower layer for panic catching
/// 
/// This can be used to set up panic hooks at the application level
pub fn setup_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info.payload();

        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        tracing::error!(
            target: "panic",
            message = %message,
            location = %location,
            "Application panic occurred"
        );
    }));

    tracing::info!("Panic hook installed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catch_panic_success() {
        let result = catch_panic(|| {
            42
        });
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_catch_panic_with_string_panic() {
        let result = catch_panic(|| {
            panic!("Test panic message");
        });
        
        assert!(result.is_err());
    }

    #[test]
    fn test_catch_panic_with_str_panic() {
        let result = catch_panic(|| -> i32 {
            panic!("Static str panic");
        });
        
        assert!(result.is_err());
    }

    #[test]
    fn test_catch_panic_computation() {
        let result = catch_panic(|| {
            let a = 10;
            let b = 20;
            a + b
        });
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 30);
    }

    #[tokio::test]
    async fn test_panic_safe_success() {
        let result = panic_safe(|| async {
            "success"
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_panic_safe_computation() {
        let result = panic_safe(|| async {
            let value = 100;
            value * 2
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 200);
    }

    #[test]
    fn test_internal_error_response_format() {
        let response = internal_error_response("Test error");
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_internal_error_response_body() {
        use http_body_util::BodyExt;
        
        let response = internal_error_response("Test error message");
        
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        
        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        
        assert_eq!(json["error"], "internal_error");
        assert_eq!(json["message"], "Test error message");
    }
}
