//! Adapter-level middleware for webshelf-axum.
//!
//! These middleware functions are framework-specific (use axum `Request`, `Next`, `State`).
//! Framework-agnostic business logic (JWT validation, token version checking) is
//! delegated to `webshelf_runtime::middleware` and `webshelf_runtime::auth`.

use axum::{
    Json,
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use serde_json::json;
use std::net::SocketAddr;

use webshelf_runtime::{AuthUser, MiddlewareState, RateLimitGuard, validate_jwt};

/// Authentication middleware — validates JWT from `Authorization` header or `webshelf_jwt` cookie.
/// Generic over `S: MiddlewareState` to avoid circular dependency on `AppState`.
/// Skips authentication for `/health` (which is inside a `/api` nest, so path is `/health`).
pub async fn auth_middleware<S: MiddlewareState + 'static>(
    State(state): State<S>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Skip authentication for public health endpoint.
    // Check both with and without the /api prefix to make the middleware
    // position-independent: when layered inside the /api nest, axum
    // strips the prefix so path is /health; when layered outside, the
    // prefix is preserved so path is /api/health.
    if path == "/health" || path == "/api/health" {
        return next.run(request).await;
    }

    let jwt_secret = state.jwt_secret();

    // Extract token from Authorization header or webshelf_jwt cookie
    let token = match extract_bearer_token(&request) {
        Some(token) => token,
        None => match extract_jwt_cookie(&request) {
            Some(token) => token,
            None => return unauthorized_response("Missing or invalid Authorization header"),
        },
    };

    // Validate token
    match validate_jwt(&token, jwt_secret) {
        Ok(claims) => {
            let user_id: i64 = match claims.sub.parse() {
                Ok(id) => id,
                Err(_) => {
                    tracing::warn!("Invalid user ID format in token: {}", claims.sub);
                    return unauthorized_response("Invalid or expired token");
                }
            };

            match state
                .check_token_version(user_id, claims.token_version)
                .await
            {
                Ok(()) => {
                    let auth_user = AuthUser::from(claims);
                    request.extensions_mut().insert(auth_user);
                    next.run(request).await
                }
                Err(e) => {
                    tracing::warn!("Token version validation failed: {}", e);
                    unauthorized_response("Invalid or expired token")
                }
            }
        }
        Err(e) => {
            tracing::warn!("Token validation failed: {}", e);
            unauthorized_response("Invalid or expired token")
        }
    }
}

/// Require admin role middleware — returns 403 if the authenticated user is not an admin or system.
pub async fn require_admin(request: Request, next: Next) -> Response {
    let auth_user = match request.extensions().get::<AuthUser>() {
        Some(user) => user,
        None => return unauthorized_response("Authentication required"),
    };

    if auth_user.role != "admin" && auth_user.role != "system" {
        return forbidden_response("Admin privileges required");
    }

    next.run(request).await
}

/// Axum middleware that catches panics and returns 500 Internal Server Error.
pub async fn panic_middleware(request: Request, next: Next) -> Response {
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

/// Apply rate-limit middleware to a route (generic over router state type `S`).
///
/// Server route builders can use this single function regardless of
/// framework, eliminating duplicated route registration code.
pub fn with_rate_limit_layer<S>(route: axum::Router<S>, guard: RateLimitGuard) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    route.layer(axum::middleware::from_fn_with_state(
        guard,
        rate_limit_middleware,
    ))
}

/// Generic rate‑limiting middleware for auth endpoints.
pub async fn rate_limit_middleware(
    State(guard): State<RateLimitGuard>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !guard.limiter.is_available() {
        return next.run(request).await;
    }

    // 1. IP‑based check
    let ip = extract_client_ip(request.headers()).or_else(|| extract_peer_ip(request.extensions()));

    if let Some(ip) = ip {
        let ip_key = format!("{}:ip:{}", guard.key_prefix, ip);
        match guard
            .limiter
            .check(&ip_key, guard.ip_max_requests, guard.ip_window_seconds)
            .await
        {
            Ok(true) => {}
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
                    return internal_error_response("An unexpected error occurred");
                }
            }
        }
    }

    // 2. Email‑based check
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
                return internal_error_response("An unexpected error occurred");
            }
        };

        if let Some(email) = extract_email_from_body(&bytes) {
            let email_key = format!("{}:email:{}", guard.key_prefix, email);
            match guard
                .limiter
                .check(&email_key, email_max, guard.email_window_seconds)
                .await
            {
                Ok(true) => {}
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
                        return internal_error_response("An unexpected error occurred");
                    }
                }
            }
        }

        let body = Body::from(bytes);
        let request = Request::from_parts(parts, body);
        next.run(request).await
    } else {
        next.run(request).await
    }
}

fn extract_bearer_token(request: &Request) -> Option<String> {
    let auth_header = request.headers().get(http::header::AUTHORIZATION)?;
    let auth_value = auth_header.to_str().ok()?;

    const BEARER_PREFIX: &[u8] = b"bearer ";
    if auth_value.len() <= BEARER_PREFIX.len() {
        return None;
    }
    if !auth_value.as_bytes()[..BEARER_PREFIX.len()].eq_ignore_ascii_case(BEARER_PREFIX) {
        return None;
    }
    Some(auth_value[BEARER_PREFIX.len()..].to_string())
}

fn extract_jwt_cookie(request: &Request) -> Option<String> {
    let cookie_header = request.headers().get(http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;

    cookie_str
        .split(';')
        .map(str::trim)
        .filter_map(|s| cookie::Cookie::parse(s).ok())
        .find(|c| c.name() == "webshelf_jwt")
        .map(|c| c.value().to_string())
}

fn extract_client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
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

fn extract_peer_ip(extensions: &axum::http::Extensions) -> Option<String> {
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
}

fn extract_email_from_body(bytes: &[u8]) -> Option<String> {
    // Try JSON first
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(bytes)
        && let Some(email) = val.get("email")?.as_str()
    {
        return Some(email.to_lowercase());
    }
    // Fallback to form-encoded (login endpoint accepts both content types)
    serde_urlencoded::from_bytes::<std::collections::HashMap<String, String>>(bytes)
        .ok()?
        .remove("email")
        .map(|e| e.to_lowercase())
}

fn unauthorized_response(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized", "message": message})),
    )
        .into_response()
}

fn forbidden_response(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"error": "forbidden", "message": message})),
    )
        .into_response()
}

fn rate_limited_response() -> Response {
    (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": "rate_limited", "message": "Too many requests. Please try again later."}))).into_response()
}

fn internal_error_response(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "internal_error", "message": message})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;

    // ── extract_bearer_token tests ───────────────────────────

    #[test]
    fn extract_bearer_token_success() {
        let req = Request::builder()
            .header("authorization", "Bearer my-token")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("my-token".to_string()));
    }

    #[test]
    fn extract_bearer_token_lowercase_bearer() {
        let req = Request::builder()
            .header("authorization", "bearer my-token")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("my-token".to_string()));
    }

    #[test]
    fn extract_bearer_token_mixed_case() {
        let req = Request::builder()
            .header("authorization", "BEARER token-value")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("token-value".to_string()));
    }

    #[test]
    fn extract_bearer_token_missing_header() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert!(extract_bearer_token(&req).is_none());
    }

    #[test]
    fn extract_bearer_token_wrong_scheme() {
        let req = Request::builder()
            .header("authorization", "Basic dXNlcjpwYXNz")
            .body(Body::empty())
            .unwrap();
        assert!(extract_bearer_token(&req).is_none());
    }

    #[test]
    fn extract_bearer_token_empty_value() {
        // "Bearer " is exactly BEARER_PREFIX.len() -> returns None
        let req = Request::builder()
            .header("authorization", "Bearer ")
            .body(Body::empty())
            .unwrap();
        assert!(extract_bearer_token(&req).is_none());
    }

    // ── extract_jwt_cookie tests ─────────────────────────────

    #[test]
    fn extract_jwt_cookie_success() {
        let req = Request::builder()
            .header("cookie", "webshelf_jwt=my-jwt-token; other=cookie")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_jwt_cookie(&req), Some("my-jwt-token".to_string()));
    }

    #[test]
    fn extract_jwt_cookie_multiple_cookies() {
        let req = Request::builder()
            .header("cookie", "session=abc; webshelf_jwt=found-token; lang=en")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_jwt_cookie(&req), Some("found-token".to_string()));
    }

    #[test]
    fn extract_jwt_cookie_missing_header() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert!(extract_jwt_cookie(&req).is_none());
    }

    #[test]
    fn extract_jwt_cookie_no_jwt_cookie() {
        let req = Request::builder()
            .header("cookie", "session=abc")
            .body(Body::empty())
            .unwrap();
        assert!(extract_jwt_cookie(&req).is_none());
    }
}
