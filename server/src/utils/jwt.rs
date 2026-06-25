use std::time::{SystemTime, UNIX_EPOCH};

pub use webshelf_runtime::JwtClaims;

/// Generate a new JWT token with issuer and audience claims
pub fn generate_token(
    user_id: &str,
    role: &str,
    secret: &str,
    expiry_seconds: u64,
    remember: bool,
    token_version: i32,
) -> anyhow::Result<String> {
    use anyhow::Context;
    use jsonwebtoken::{EncodingKey, Header, encode};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("Failed to get current time")?;

    let claims = JwtClaims {
        sub: user_id.to_string(),
        exp: now.as_secs() + expiry_seconds,
        iat: now.as_secs(),
        iss: "webshelf-server".to_string(),
        aud: "webshelf".to_string(),
        role: role.to_string(),
        token_version,
        remember,
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

    #[test]
    fn generate_token_roundtrip() {
        let token = generate_token("42", "admin", "secret-key", 3600, false, 1).unwrap();
        let claims = webshelf_runtime::validate_jwt(&token, "secret-key").unwrap();
        assert_eq!(claims.sub, "42");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.iss, "webshelf-server");
        assert_eq!(claims.aud, "webshelf");
        assert_eq!(claims.token_version, 1);
        assert!(!claims.remember);
    }

    #[test]
    fn generate_token_with_remember() {
        let token = generate_token("1", "user", "secret", 7200, true, 5).unwrap();
        let claims = webshelf_runtime::validate_jwt(&token, "secret").unwrap();
        assert_eq!(claims.sub, "1");
        assert_eq!(claims.token_version, 5);
        assert!(claims.remember);
    }

    #[test]
    fn generate_token_wrong_secret_fails_validation() {
        let token = generate_token("1", "user", "correct_secret", 3600, false, 1).unwrap();
        let result = webshelf_runtime::validate_jwt(&token, "wrong_secret");
        assert!(result.is_err());
    }
}
