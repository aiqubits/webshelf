use axum::{
    body::Body,
    extract::{State, connect_info::ConnectInfo},
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use http_body_util::BodyExt;
use serde_json::Value;

use distributed_ratelimit::RedisRateLimiter;

use crate::utils::error::ErrorResponse;

/// Per‑endpoint rate‑limit parameters.
///
/// Each auth route that needs rate limiting gets its own `RateLimitGuard`
/// describing the IP‑based threshold and (optionally) an email‑based
/// threshold.  The middleware is fully generic — just plug different
/// `RateLimitGuard` values.
#[derive(Clone)]
pub struct RateLimitGuard {
    /// Shared Redis rate limiter.
    pub limiter: RedisRateLimiter,

    /// Max requests from a single IP within `ip_window_seconds`.
    pub ip_max_requests: u64,

    /// Window length for IP‑based limiting (seconds).
    pub ip_window_seconds: u64,

    /// Max requests for a single email within `email_window_seconds`.
    /// `None` means skip email‑based checks (avoids reading the body).
    pub email_max_requests: Option<u64>,

    /// Window length for email‑based limiting (seconds).
    pub email_window_seconds: u64,

    /// Prefix for Redis keys, e.g. `"login"` → `ratelimit:login:ip:…`.
    pub key_prefix: &'static str,
}

// ── Middleware ────────────────────────────────────────────────────────────

/// Generic rate‑limiting middleware for auth endpoints.
///
/// Behaviour is driven entirely by `State<RateLimitGuard>`:
/// - Always checks IP‑based limits.
/// - Only checks email‑based limits when `email_max_requests` is `Some`.
pub async fn rate_limit_middleware(
    State(guard): State<RateLimitGuard>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !guard.limiter.is_available() {
        return next.run(request).await;
    }

    // ── 1. IP‑based check (all endpoints) ───────────────────────────
    let ip = extract_client_ip(request.headers()).or_else(|| extract_peer_ip(request.extensions()));

    if let Some(ip) = ip {
        let ip_key = format!("{}:ip:{}", guard.key_prefix, ip);
        match guard
            .limiter
            .check(&ip_key, guard.ip_max_requests, guard.ip_window_seconds)
            .await
        {
            Ok(true) => {} // allowed
            Ok(false) => {
                tracing::warn!("Rate limit exceeded (IP) for {}: {}", guard.key_prefix, ip);
                return rate_limited_response();
            }
            Err(e) => {
                tracing::error!(
                    "Rate‑limit Redis error (IP) for {}: {:?}",
                    guard.key_prefix,
                    e
                );
                if !guard.limiter.fail_open() {
                    return internal_error_response();
                }
            }
        }
    }

    // ── 2. Email‑based check (only when configured) ─────────────────
    if let Some(email_max) = guard.email_max_requests {
        let (parts, body) = request.into_parts();
        let bytes = match body.collect().await {
            Ok(c) => c.to_bytes(),
            Err(e) => {
                tracing::error!(
                    "Failed to read body for rate limiting ({}): {:?}",
                    guard.key_prefix,
                    e
                );
                return internal_error_response();
            }
        };

        if let Some(email) = extract_email_from_body(&bytes) {
            let email_key = format!("{}:email:{}", guard.key_prefix, email);
            match guard
                .limiter
                .check(&email_key, email_max, guard.email_window_seconds)
                .await
            {
                Ok(true) => {} // allowed
                Ok(false) => {
                    tracing::warn!(
                        "Rate limit exceeded (email) for {}: {}",
                        guard.key_prefix,
                        email
                    );
                    return rate_limited_response();
                }
                Err(e) => {
                    tracing::error!(
                        "Rate‑limit Redis error (email) for {}: {:?}",
                        guard.key_prefix,
                        e
                    );
                    if !guard.limiter.fail_open() {
                        return internal_error_response();
                    }
                }
            }
        }

        let body = Body::from(bytes);
        let request = Request::from_parts(parts, body);
        next.run(request).await
    } else {
        // No email check — forward request unmodified.
        next.run(request).await
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Extract the client IP from headers.
///
/// Priority:
/// 1. `X-Forwarded-For` (first IP in the comma‑separated list)
/// 2. `X-Real-IP`
fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-forwarded-for")
        && let Ok(value) = value.to_str()
        && let Some(ip) = value.split(',').next()
    {
        let ip = ip.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }

    if let Some(value) = headers.get("x-real-ip")
        && let Ok(value) = value.to_str()
    {
        let ip = value.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }

    None
}

/// Extract the IP from the TCP peer address (injected by `ConnectInfo`).
///
/// This is a fallback when no proxy headers are present (e.g. direct
/// connections in development or when the reverse proxy does not forward
/// headers).
fn extract_peer_ip(extensions: &http::Extensions) -> Option<String> {
    extensions
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
}

/// Try to extract the `email` field from a JSON request body.
fn extract_email_from_body(bytes: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| v.get("email")?.as_str().map(|s| s.to_lowercase()))
}

// ── Response builders ────────────────────────────────────────────────────

fn rate_limited_response() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorResponse::new(
            "rate_limited",
            "Too many requests. Please try again later.",
        )),
    )
        .into_response()
}

fn internal_error_response() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new(
            "internal_error",
            "An unexpected error occurred",
        )),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_email_from_body_success() {
        let body = br#"{"email": "User@Example.COM", "password": "secret"}"#;
        assert_eq!(
            extract_email_from_body(body),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn test_extract_email_from_body_missing_field() {
        let body = br#"{"name": "test"}"#;
        assert_eq!(extract_email_from_body(body), None);
    }

    #[test]
    fn test_extract_email_from_body_invalid_json() {
        let body = b"not json";
        assert_eq!(extract_email_from_body(body), None);
    }

    #[test]
    fn test_extract_client_ip_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(extract_client_ip(&headers), Some("1.2.3.4".to_string()));
    }

    #[test]
    fn test_extract_client_ip_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.1".parse().unwrap());
        assert_eq!(extract_client_ip(&headers), Some("10.0.0.1".to_string()));
    }

    #[test]
    fn test_extract_client_ip_prefers_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1".parse().unwrap());
        headers.insert("x-real-ip", "2.2.2.2".parse().unwrap());
        assert_eq!(extract_client_ip(&headers), Some("1.1.1.1".to_string()));
    }

    #[test]
    fn test_extract_client_ip_no_headers() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_ip(&headers), None);
    }

    // ── extract_peer_ip ────────────────────────────────────────────

    #[test]
    fn test_extract_peer_ip_with_connect_info() {
        use axum::extract::connect_info::ConnectInfo;

        let mut extensions = http::Extensions::new();
        extensions.insert(ConnectInfo(
            "127.0.0.1:8080".parse::<std::net::SocketAddr>().unwrap(),
        ));
        assert_eq!(extract_peer_ip(&extensions), Some("127.0.0.1".to_string()));
    }

    #[test]
    fn test_extract_peer_ip_missing() {
        let extensions = http::Extensions::new();
        assert_eq!(extract_peer_ip(&extensions), None);
    }

    // ── Response builders ──────────────────────────────────────────

    #[tokio::test]
    async fn test_rate_limited_response_format() {
        let response = rate_limited_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"], "rate_limited");
        assert_eq!(
            json["message"],
            "Too many requests. Please try again later."
        );
    }

    #[tokio::test]
    async fn test_internal_error_response_format() {
        let response = internal_error_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"], "internal_error");
        assert_eq!(json["message"], "An unexpected error occurred");
    }

    // ── Middleware integration (disabled limiter) ──────────────────

    #[tokio::test]
    async fn test_disabled_limiter_passes_through_ip_check() {
        use axum::Router;
        use axum::routing::get;
        use distributed_ratelimit::RateLimitConfig;
        use tower::ServiceExt;

        let guard = RateLimitGuard {
            limiter: RedisRateLimiter::disabled(RateLimitConfig::default()),
            ip_max_requests: 0, // would block immediately if active
            ip_window_seconds: 60,
            email_max_requests: None,
            email_window_seconds: 60,
            key_prefix: "test",
        };

        let app = Router::new().route("/", get(|| async { "passed" })).layer(
            axum::middleware::from_fn_with_state(guard, rate_limit_middleware),
        );

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(bytes.as_ref(), b"passed");
    }

    #[tokio::test]
    async fn test_disabled_limiter_ignores_email_check() {
        use axum::Router;
        use axum::routing::post;
        use distributed_ratelimit::RateLimitConfig;
        use tower::ServiceExt;

        let guard = RateLimitGuard {
            limiter: RedisRateLimiter::disabled(RateLimitConfig::default()),
            ip_max_requests: 100,
            ip_window_seconds: 60,
            email_max_requests: Some(0), // would block immediately if active
            email_window_seconds: 60,
            key_prefix: "test",
        };

        let app = Router::new()
            .route("/", post(|_: String| async { "passed" }))
            .layer(axum::middleware::from_fn_with_state(
                guard,
                rate_limit_middleware,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"email": "test@example.com"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(bytes.as_ref(), b"passed");
    }
}
