use axum::{
    Json,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::utils::error::ErrorResponse;

/// Create an internal error response
fn internal_error_response(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new("internal_error", message)),
    )
        .into_response()
}

/// Axum middleware that catches panics in downstream handlers
/// and returns a 500 Internal Server Error response instead.
pub async fn panic_middleware(request: Request, next: Next) -> Response {
    // Spawn the inner handler on a separate task so that if it panics,
    // the panic is contained and the server process stays alive.
    let response = tokio::spawn(async move { next.run(request).await }).await;

    match response {
        Ok(resp) => resp,
        Err(err) => {
            let panic_message = if err.is_panic() {
                if let Some(s) = err.try_into_panic().ok().and_then(|p| {
                    p.downcast_ref::<String>()
                        .cloned()
                        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                }) {
                    s
                } else {
                    "Unknown panic occurred".to_string()
                }
            } else if err.is_cancelled() {
                "Task was cancelled".to_string()
            } else {
                "Task failed".to_string()
            };

            tracing::error!("Panic caught in middleware: {}", panic_message);
            internal_error_response("An unexpected error occurred")
        }
    }
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

    #[tokio::test]
    async fn test_panic_middleware_success() {
        use axum::{Router, routing::get};
        use tower::ServiceExt;

        async fn ok_handler() -> &'static str {
            "success"
        }

        let app = Router::new()
            .route("/", get(ok_handler))
            .layer(axum::middleware::from_fn(panic_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        use http_body_util::BodyExt;
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(body, "success");
    }

    #[tokio::test]
    async fn test_panic_middleware_catches_panic() {
        use axum::{Router, routing::get};
        use tower::ServiceExt;

        async fn panicking_handler() -> &'static str {
            panic!("Test panic!");
        }

        let app = Router::new()
            .route("/", get(panicking_handler))
            .layer(axum::middleware::from_fn(panic_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
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
