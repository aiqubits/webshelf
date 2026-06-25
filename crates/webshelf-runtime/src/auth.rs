use serde::{Deserialize, Serialize};

/// JWT claims used by webshelf
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: String,
    pub exp: u64,
    pub iat: u64,
    pub iss: String,
    pub aud: String,
    pub role: String,
    pub token_version: i32,
    /// Whether the original login had "remember me" enabled
    #[serde(default)]
    pub remember: bool,
}

/// Authenticated user information extracted from JWT.
/// Shared between middleware and handlers via request context.
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
    /// Whether the original login had "remember me" enabled
    pub remember: bool,
}

impl From<JwtClaims> for AuthUser {
    fn from(claims: JwtClaims) -> Self {
        Self {
            user_id: claims.sub,
            role: claims.role,
            exp: claims.exp,
            iat: claims.iat,
            token_version: claims.token_version,
            remember: claims.remember,
        }
    }
}

/// Validate JWT with strict signature, algorithm, expiration, issuer, and audience checks.
pub fn validate_jwt(token: &str, secret: &str) -> Result<JwtClaims, String> {
    use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.leeway = 5;
    validation.set_issuer(&["webshelf-server"]);
    validation.set_audience(&["webshelf"]);

    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};

    fn test_claims() -> JwtClaims {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        JwtClaims {
            sub: "123".to_string(),
            exp: now + 3600,
            iat: now,
            iss: "webshelf-server".to_string(),
            aud: "webshelf".to_string(),
            role: "admin".to_string(),
            token_version: 1,
            remember: false,
        }
    }

    fn create_token(claims: &JwtClaims, secret: &str) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn validate_jwt_valid_token() {
        let claims = test_claims();
        let token = create_token(&claims, "my_secret");
        let result = validate_jwt(&token, "my_secret").unwrap();
        assert_eq!(result.sub, "123");
        assert_eq!(result.role, "admin");
    }

    #[test]
    fn validate_jwt_wrong_secret() {
        let claims = test_claims();
        let token = create_token(&claims, "correct_secret");
        let result = validate_jwt(&token, "wrong_secret");
        assert!(result.is_err());
    }

    #[test]
    fn validate_jwt_expired_token() {
        let mut claims = test_claims();
        claims.exp = 1; // Expired long ago
        let token = create_token(&claims, "my_secret");
        let result = validate_jwt(&token, "my_secret");
        assert!(result.is_err());
    }

    #[test]
    fn validate_jwt_wrong_issuer() {
        let mut claims = test_claims();
        claims.iss = "wrong-issuer".to_string();
        let token = create_token(&claims, "my_secret");
        let result = validate_jwt(&token, "my_secret");
        assert!(result.is_err());
    }

    #[test]
    fn validate_jwt_wrong_audience() {
        let mut claims = test_claims();
        claims.aud = "wrong-audience".to_string();
        let token = create_token(&claims, "my_secret");
        let result = validate_jwt(&token, "my_secret");
        assert!(result.is_err());
    }

    #[test]
    fn validate_jwt_malformed_token() {
        let result = validate_jwt("not-a-valid-jwt", "my_secret");
        assert!(result.is_err());
    }
}
