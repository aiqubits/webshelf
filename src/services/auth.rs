use crate::middleware::auth::generate_token;
use crate::models::user::{Entity as UserEntity, Model as UserModel};
use crate::utils::password::{hash_password, verify_password};
use anyhow::{anyhow, Context, Result};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

/// Authentication service for user login and token management
pub struct AuthService {
    db: DatabaseConnection,
    jwt_secret: String,
    jwt_expiry_seconds: u64,
}

/// Login request payload
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response with token
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
}

/// Token refresh request
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub token: String,
}

impl AuthService {
    /// Create a new authentication service
    pub fn new(db: DatabaseConnection, jwt_secret: String, jwt_expiry_seconds: u64) -> Self {
        Self {
            db,
            jwt_secret,
            jwt_expiry_seconds,
        }
    }

    /// Authenticate user with email and password
    pub async fn login(&self, request: LoginRequest) -> Result<LoginResponse> {
        // Find user by email
        let user = UserEntity::find()
            .filter(crate::models::user::Column::Email.eq(&request.email))
            .one(&self.db)
            .await
            .context("Failed to query user")?
            .ok_or_else(|| anyhow!("Invalid email or password"))?;

        // Verify password
        let is_valid = verify_password(&request.password, &user.password_hash)
            .context("Failed to verify password")?;

        if !is_valid {
            return Err(anyhow!("Invalid email or password"));
        }

        // Generate JWT token
        let token = generate_token(
            &user.id.to_string(),
            &user.role,
            &self.jwt_secret,
            self.jwt_expiry_seconds,
        )
        .context("Failed to generate token")?;

        tracing::info!("User {} logged in successfully", user.email);

        Ok(LoginResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: self.jwt_expiry_seconds,
            user_id: user.id.to_string(),
            role: user.role,
        })
    }

    /// Validate a token and return user info
    pub async fn validate_token(&self, token: &str) -> Result<UserModel> {
        use crate::middleware::auth::Claims;
        use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &validation,
        )
        .context("Invalid token")?;

        let user_id = uuid::Uuid::parse_str(&token_data.claims.sub)
            .context("Invalid user ID in token")?;

        let user = UserEntity::find_by_id(user_id)
            .one(&self.db)
            .await
            .context("Failed to query user")?
            .ok_or_else(|| anyhow!("User not found"))?;

        Ok(user)
    }

    /// Hash a password for storage
    pub fn hash_password(password: &str) -> Result<String> {
        hash_password(password)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_request_deserialization() {
        let json = r#"{
            "email": "user@example.com",
            "password": "password123"
        }"#;
        
        let request: LoginRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.email, "user@example.com");
        assert_eq!(request.password, "password123");
    }

    #[test]
    fn test_login_response_serialization() {
        let response = LoginResponse {
            token: "jwt.token.here".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            user_id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            role: "user".to_string(),
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("jwt.token.here"));
        assert!(json.contains("Bearer"));
        assert!(json.contains("3600"));
        assert!(json.contains("user"));
    }

    #[test]
    fn test_refresh_token_request_deserialization() {
        let json = r#"{
            "token": "old.jwt.token"
        }"#;
        
        let request: RefreshTokenRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.token, "old.jwt.token");
    }

    #[test]
    fn test_hash_password_wrapper() {
        let password = "SecurePassword123!";
        let hash = AuthService::hash_password(password).unwrap();
        
        assert!(!hash.is_empty());
        assert_ne!(hash, password);
        assert!(hash.starts_with("$argon2"));
    }
}
