use axum::{
    extract::Request,
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

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
}

impl From<Claims> for AuthUser {
    fn from(claims: Claims) -> Self {
        Self {
            user_id: claims.sub,
            role: claims.role,
            exp: claims.exp,
            iat: claims.iat,
        }
    }
}

/// Authentication middleware
/// 
/// Validates JWT token from Authorization header and injects AuthUser into request extensions.
/// Skips authentication for paths starting with /api/public or /api/health.
pub async fn auth_middleware(mut request: Request, next: Next) -> Response {
    let path = request.uri().path();

    // Skip authentication for public endpoints
    if path.starts_with("/api/public") || path.starts_with("/api/health") {
        return next.run(request).await;
    }

    // Extract JWT secret from request extensions (should be set by app state)
    let jwt_secret = match request.extensions().get::<JwtSecret>() {
        Some(secret) => secret.0.clone(),
        None => {
            tracing::error!("JWT secret not found in request extensions");
            return unauthorized_response("Server configuration error");
        }
    };

    // Extract token from Authorization header
    let token = match extract_bearer_token(&request) {
        Some(token) => token,
        None => {
            return unauthorized_response("Missing or invalid Authorization header");
        }
    };

    // Validate token with strict checks
    match validate_token(&token, &jwt_secret) {
        Ok(claims) => {
            // Check if token is expired
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if claims.exp < now {
                return unauthorized_response("Token has expired");
            }

            // Inject authenticated user into request extensions
            let auth_user = AuthUser::from(claims);
            request.extensions_mut().insert(auth_user);

            next.run(request).await
        }
        Err(e) => {
            tracing::warn!("Token validation failed: {}", e);
            unauthorized_response(&format!("Invalid token: {}", e))
        }
    }
}

/// JWT secret wrapper for request extensions
#[derive(Clone)]
pub struct JwtSecret(pub String);

/// Extract bearer token from Authorization header
fn extract_bearer_token(request: &Request) -> Option<String> {
    let auth_header = request.headers().get(AUTHORIZATION)?;
    let auth_str = auth_header.to_str().ok()?;

    if auth_str.starts_with("Bearer ") {
        Some(auth_str[7..].to_string())
    } else {
        None
    }
}

/// Validate JWT token with strict signature, algorithm, and expiration validation
fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    Ok(token_data.claims)
}

/// Create an unauthorized response
fn unauthorized_response(message: &str) -> Response {
    #[derive(Serialize)]
    struct ErrorBody {
        error: String,
        message: String,
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorBody {
            error: "unauthorized".to_string(),
            message: message.to_string(),
        }),
    )
        .into_response()
}

/// Generate a new JWT token
pub fn generate_token(
    user_id: &str,
    role: &str,
    secret: &str,
    expiry_seconds: u64,
) -> anyhow::Result<String> {
    use anyhow::Context;
    use jsonwebtoken::{encode, EncodingKey, Header};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("Failed to get current time")?;

    let claims = Claims {
        sub: user_id.to_string(),
        exp: now.as_secs() + expiry_seconds,
        iat: now.as_secs(),
        role: role.to_string(),
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
        let token = generate_token(TEST_USER_ID, TEST_ROLE, TEST_SECRET, 3600).unwrap();
        assert!(!token.is_empty());
        assert!(token.split('.').count() == 3); // JWT has 3 parts
    }

    #[test]
    fn test_validate_token_success() {
        let token = generate_token(TEST_USER_ID, TEST_ROLE, TEST_SECRET, 3600).unwrap();
        let claims = validate_token(&token, TEST_SECRET).unwrap();
        
        assert_eq!(claims.sub, TEST_USER_ID);
        assert_eq!(claims.role, TEST_ROLE);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_validate_token_wrong_secret() {
        let token = generate_token(TEST_USER_ID, TEST_ROLE, TEST_SECRET, 3600).unwrap();
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
        };
        
        let auth_user = AuthUser::from(claims);
        
        assert_eq!(auth_user.user_id, TEST_USER_ID);
        assert_eq!(auth_user.role, TEST_ROLE);
        assert_eq!(auth_user.exp, now + 3600);
        assert_eq!(auth_user.iat, now);
    }

    #[test]
    fn test_extract_bearer_token_success() {
        use axum::http::{HeaderValue, Request};
        use axum::body::Body;
        
        let mut request = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-123"),
        );
        
        let token = extract_bearer_token(&request).unwrap();
        assert_eq!(token, "test-token-123");
    }

    #[test]
    fn test_extract_bearer_token_missing_header() {
        use axum::body::Body;
        
        let request = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        
        let token = extract_bearer_token(&request);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_bearer_token_wrong_scheme() {
        use axum::http::{HeaderValue, Request};
        use axum::body::Body;
        
        let mut request = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("Basic dGVzdDp0ZXN0"),
        );
        
        let token = extract_bearer_token(&request);
        assert!(token.is_none());
    }

    #[test]
    fn test_token_expiry() {
        // Generate token that expires in 1 second
        let token = generate_token(TEST_USER_ID, TEST_ROLE, TEST_SECRET, 1).unwrap();
        
        // Should be valid immediately
        let claims = validate_token(&token, TEST_SECRET).unwrap();
        assert!(claims.exp > claims.iat);
        
        // Wait for expiry (in real scenario, validation middleware would check this)
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        // Token is still decodable, but exp check would fail in middleware
        let claims = validate_token(&token, TEST_SECRET).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(claims.exp < now);
    }
}
