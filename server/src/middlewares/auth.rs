use anyhow::Context;
use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppState;
use crate::repositories::user::Entity as UserEntity;
use crate::utils::error::ErrorResponse;

/// Authenticated user information extracted from JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    /// User ID (subject)
    pub user_id: String,
    /// User role for RBAC
    pub role: String,
    /// Token expiration timestamp
    pub exp: u64,
    /// Token issued at timestamp
    pub iat: u64,
    /// Token version for invalidation (matches user.token_version in DB)
    pub token_version: i32,
}

/// JWT Claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Expiration time
    pub exp: u64,
    /// Issued at
    pub iat: u64,
    /// User role for RBAC
    pub role: String,
    /// Token version for invalidation (matches user.token_version in DB)
    pub token_version: i32,
}

impl From<Claims> for AuthUser {
    fn from(claims: Claims) -> Self {
        Self {
            user_id: claims.sub,
            role: claims.role,
            exp: claims.exp,
            iat: claims.iat,
            token_version: claims.token_version,
        }
    }
}

/// Authentication middleware
///
/// Validates JWT token from Authorization header and injects AuthUser into request extensions.
/// Skips authentication for paths starting with /api/public or /api/health.
///
/// Also verifies the token's token_version against the user's current token_version in the
/// database, enabling token invalidation when a user changes their password.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Skip authentication for public endpoints
    // Use precise matching: /api/public/ for public routes, /api/health for health check
    if path == "/api/health" || path.starts_with("/api/public/") {
        return next.run(request).await;
    }

    // Use JWT secret from app state (injected via from_fn_with_state)
    let jwt_secret = &state.config.jwt_secret;

    // Extract token from Authorization header
    let token = match extract_bearer_token(&request) {
        Some(token) => token,
        None => {
            return unauthorized_response("Missing or invalid Authorization header");
        }
    };

    // Validate token with strict checks
    match validate_token(&token, jwt_secret) {
        Ok(claims) => {
            // Verify token_version against the database
            let user_id = uuid::Uuid::parse_str(&claims.sub);
            let user_id = match user_id {
                Ok(id) => id,
                Err(_) => {
                    tracing::warn!("Invalid user ID format in token: {}", claims.sub);
                    return unauthorized_response("Invalid or expired token");
                }
            };

            match verify_token_version(&state.db, user_id, claims.token_version).await {
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

/// Verify that the token's token_version matches the user's current token_version.
/// Queries the database directly — the PK lookup is O(1) and fast enough
/// that caching provides negligible benefit while introducing cache-invalidation
/// race conditions (token_version changes on password/role changes).
///
/// TODO: For high-throughput deployments (1k+ req/s), consider a Redis-based
/// (user_id → token_version) cache with per-user TTL equal to JWT remaining
/// lifetime, falling back to DB on cache miss.
async fn verify_token_version(
    db: &sea_orm::DatabaseConnection,
    user_id: uuid::Uuid,
    token_version: i32,
) -> anyhow::Result<()> {
    let user = UserEntity::find_by_id(user_id)
        .one(db)
        .await
        .context("Failed to query user for token version check")?;

    let user = user.ok_or_else(|| anyhow::anyhow!("User not found"))?;

    if user.token_version != token_version {
        return Err(anyhow::anyhow!(
            "Token version mismatch (token was invalidated by password change)"
        ));
    }

    Ok(())
}

/// Extract bearer token from Authorization header.
///
/// The "Bearer" scheme prefix is matched case-insensitively per RFC 6750,
/// so "Bearer", "bearer", "BEARER", etc. are all accepted.
fn extract_bearer_token(request: &Request) -> Option<String> {
    let auth_header = request.headers().get(AUTHORIZATION)?;
    let auth_str = auth_header.to_str().ok()?;

    const BEARER_PREFIX: &str = "bearer ";
    if auth_str.len() > BEARER_PREFIX.len()
        && auth_str[..BEARER_PREFIX.len()].eq_ignore_ascii_case(BEARER_PREFIX)
    {
        Some(auth_str[BEARER_PREFIX.len()..].to_string())
    } else {
        None
    }
}

/// Validate JWT token with strict signature, algorithm, and expiration validation
fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.leeway = 5;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    Ok(token_data.claims)
}

/// Create an unauthorized response (401)
fn unauthorized_response(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse::new("unauthorized", message)),
    )
        .into_response()
}

/// Create a forbidden response (403)
fn forbidden_response(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("forbidden", message)),
    )
        .into_response()
}

/// Require admin role middleware — returns 403 if the authenticated user is not an admin.
///
/// The `system` role (super-admin) also passes this check.
/// Apply this middleware to routes that require admin privileges.
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

/// Generate a new JWT token
pub fn generate_token(
    user_id: &str,
    role: &str,
    secret: &str,
    expiry_seconds: u64,
    token_version: i32,
) -> anyhow::Result<String> {
    use anyhow::Context;
    use jsonwebtoken::{EncodingKey, Header, encode};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("Failed to get current time")?;

    let claims = Claims {
        sub: user_id.to_string(),
        exp: now.as_secs() + expiry_seconds,
        iat: now.as_secs(),
        role: role.to_string(),
        token_version,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("Failed to encode JWT token")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-key-for-testing";
    const TEST_USER_ID: &str = "123e4567-e89b-12d3-a456-426614174000";
    const TEST_ROLE: &str = "admin";

    #[test]
    fn test_generate_token_success() {
        let token = generate_token(TEST_USER_ID, TEST_ROLE, TEST_SECRET, 3600, 1).unwrap();
        assert!(!token.is_empty());
        assert!(token.split('.').count() == 3); // JWT has 3 parts
    }

    #[test]
    fn test_validate_token_success() {
        let token = generate_token(TEST_USER_ID, TEST_ROLE, TEST_SECRET, 3600, 1).unwrap();
        let claims = validate_token(&token, TEST_SECRET).unwrap();

        assert_eq!(claims.sub, TEST_USER_ID);
        assert_eq!(claims.role, TEST_ROLE);
        assert_eq!(claims.token_version, 1);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_validate_token_wrong_secret() {
        let token = generate_token(TEST_USER_ID, TEST_ROLE, TEST_SECRET, 3600, 1).unwrap();
        let result = validate_token(&token, "wrong-secret");

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_token_invalid_format() {
        let result = validate_token("invalid.token.format", TEST_SECRET);
        assert!(result.is_err());
    }

    #[test]
    fn test_claims_to_auth_user() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: TEST_USER_ID.to_string(),
            exp: now + 3600,
            iat: now,
            role: TEST_ROLE.to_string(),
            token_version: 2,
        };

        let auth_user = AuthUser::from(claims);

        assert_eq!(auth_user.user_id, TEST_USER_ID);
        assert_eq!(auth_user.role, TEST_ROLE);
        assert_eq!(auth_user.exp, now + 3600);
        assert_eq!(auth_user.iat, now);
        assert_eq!(auth_user.token_version, 2);
    }

    #[test]
    fn test_extract_bearer_token_success() {
        use axum::body::Body;
        use axum::http::{HeaderValue, Request};

        let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();

        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-123"),
        );

        let token = extract_bearer_token(&request).unwrap();
        assert_eq!(token, "test-token-123");
    }

    #[test]
    fn test_extract_bearer_token_lowercase() {
        use axum::body::Body;
        use axum::http::{HeaderValue, Request};

        let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();

        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("bearer test-token-lower"),
        );

        let token = extract_bearer_token(&request).unwrap();
        assert_eq!(token, "test-token-lower");
    }

    #[test]
    fn test_extract_bearer_token_mixed_case() {
        use axum::body::Body;
        use axum::http::{HeaderValue, Request};

        let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();

        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("BeArEr test-token-mixed"),
        );

        let token = extract_bearer_token(&request).unwrap();
        assert_eq!(token, "test-token-mixed");
    }

    #[test]
    fn test_extract_bearer_token_missing_header() {
        use axum::body::Body;

        let request = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let token = extract_bearer_token(&request);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_bearer_token_wrong_scheme() {
        use axum::body::Body;
        use axum::http::{HeaderValue, Request};

        let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();

        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("Basic dGVzdDp0ZXN0"),
        );

        let token = extract_bearer_token(&request);
        assert!(token.is_none());
    }

    #[test]
    fn test_token_expiry() {
        use jsonwebtoken::{EncodingKey, Header, encode};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create a token that expired 10 seconds ago (well beyond the 5s leeway)
        let expired_claims = Claims {
            sub: TEST_USER_ID.to_string(),
            exp: now - 10,
            iat: now - 70,
            role: TEST_ROLE.to_string(),
            token_version: 1,
        };

        let token = encode(
            &Header::default(),
            &expired_claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .unwrap();

        let result = validate_token(&token, TEST_SECRET);
        assert!(result.is_err(), "Expired token should be rejected");
        let err = result.unwrap_err();
        assert_eq!(
            err.kind(),
            &jsonwebtoken::errors::ErrorKind::ExpiredSignature
        );
    }
}
