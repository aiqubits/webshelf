use crate::middlewares::auth::generate_token;
use crate::repositories::user::Entity as UserEntity;
use crate::utils::password::{hash_password, verify_password};
use anyhow::Context;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

/// Typed errors for authentication operations
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid email or password")]
    InvalidCredentials,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

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

impl AuthService {
    /// Create a new authentication service
    pub fn new(db: DatabaseConnection, jwt_secret: String, jwt_expiry_seconds: u64) -> Self {
        Self {
            db,
            jwt_secret,
            jwt_expiry_seconds,
        }
    }

    /// Authenticate user with email and password.
    ///
    /// Uses constant-time comparison: always performs an Argon2 operation
    /// regardless of whether the user exists, to prevent timing-based email
    /// enumeration attacks.
    pub async fn login(&self, request: LoginRequest) -> Result<LoginResponse, AuthError> {
        let email_normalized = request.email.to_lowercase();
        let user_result = UserEntity::find()
            .filter(crate::repositories::user::Column::Email.eq(&email_normalized))
            .one(&self.db)
            .await
            .context("Failed to query user")?;

        // Constant-time: always perform an Argon2 operation regardless of
        // whether the user exists, to prevent timing-based user enumeration.
        // For non-existent users, we hash the provided password (same cost
        // as verification) and discard the result.
        let (user, is_valid) = match user_result {
            Some(user) => {
                let is_valid = verify_password(&request.password, &user.password_hash)
                    .context("Failed to verify password")?;
                (Some(user), is_valid)
            }
            None => {
                // Hash the provided password to spend equivalent CPU time.
                // The result is intentionally discarded — this is purely to
                // make the non-existent-user path as slow as the existent-user
                // path, preventing attackers from enumerating valid emails
                // by measuring response time.
                if let Err(e) = hash_password(&request.password) {
                    tracing::warn!(
                        "Honeypot password hash failed for non-existent user: {:?}",
                        e
                    );
                }
                (None, false)
            }
        };

        let user = user.ok_or(AuthError::InvalidCredentials)?;

        if !is_valid {
            return Err(AuthError::InvalidCredentials);
        }

        let token = generate_token(
            &user.id.to_string(),
            &user.role,
            &self.jwt_secret,
            self.jwt_expiry_seconds,
            user.token_version,
        )
        .context("Failed to generate token")?;

        tracing::info!("User {} logged in successfully", user.id);

        Ok(LoginResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: self.jwt_expiry_seconds,
            user_id: user.id.to_string(),
            role: user.role,
        })
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
}
