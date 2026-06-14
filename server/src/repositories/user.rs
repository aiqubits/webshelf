use sea_orm::entity::prelude::*;
use serde::Deserialize;
use serde::Serialize as SerializeTrait;

/// User database entity model
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    /// Unique user identifier
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    /// User email address (unique)
    #[sea_orm(unique)]
    pub email: String,

    /// Argon2 hashed password
    pub password_hash: String,

    /// User display name
    pub name: String,

    /// User role for RBAC (e.g., "user", "admin")
    pub role: String,

    /// Account creation timestamp
    pub created_at: DateTimeUtc,

    /// Last update timestamp
    pub updated_at: DateTimeUtc,

    /// Token version counter, incremented when password changes to invalidate old JWTs
    #[sea_orm(default_value = 1)]
    pub token_version: i32,

    /// Whether the user's email has been verified
    #[sea_orm(default_value = false)]
    pub email_verified: bool,

    /// Hash of the email verification code (argon2)
    pub verification_code_hash: Option<String>,

    /// When the verification code expires
    pub verification_code_expires_at: Option<DateTimeUtc>,

    /// When the verification code was last sent (for rate limiting)
    pub verification_code_sent_at: Option<DateTimeUtc>,

    /// Failed verification attempt counter (for brute-force protection)
    #[sea_orm(default_value = 0)]
    pub verification_failed_attempts: i32,

    /// Argon2 hash of the active password-reset token (single-use)
    pub password_reset_token_hash: Option<String>,

    /// When the password-reset token expires
    pub password_reset_expires_at: Option<DateTimeUtc>,

    /// When the password-reset email was last sent (for resend cooldown)
    pub password_reset_sent_at: Option<DateTimeUtc>,

    /// Failed password-reset attempts counter (brute-force protection)
    #[sea_orm(default_value = 0)]
    pub password_reset_failed_attempts: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// User creation input
#[derive(Debug, Deserialize)]
pub struct CreateUserInput {
    pub email: String,
    pub password: String,
    pub name: String,
}

/// User update input
#[derive(Debug, Deserialize)]
pub struct UpdateUserInput {
    pub email: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
}

/// User response (without sensitive data)
#[derive(Debug, SerializeTrait)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub email_verified: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    /// Internal token version counter — skipped in external API responses.
    #[serde(skip)]
    pub token_version: i32,
}

impl From<Model> for UserResponse {
    fn from(model: Model) -> Self {
        Self {
            id: model.id,
            email: model.email,
            name: model.name,
            role: model.role,
            email_verified: model.email_verified,
            created_at: model.created_at,
            updated_at: model.updated_at,
            token_version: model.token_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_user_response_from_model() {
        let now = Utc::now();
        let user_id = Uuid::new_v4();

        let model = Model {
            id: user_id,
            email: "test@example.com".to_string(),
            password_hash: "hashed_password".to_string(),
            name: "Test User".to_string(),
            role: "user".to_string(),
            created_at: now,
            updated_at: now,
            token_version: 1,
            email_verified: false,
            verification_code_hash: None,
            verification_code_expires_at: None,
            verification_code_sent_at: None,
            verification_failed_attempts: 0,
            password_reset_token_hash: None,
            password_reset_expires_at: None,
            password_reset_sent_at: None,
            password_reset_failed_attempts: 0,
        };

        let response = UserResponse::from(model.clone());

        assert_eq!(response.id, user_id);
        assert_eq!(response.email, "test@example.com");
        assert_eq!(response.name, "Test User");
        assert_eq!(response.role, "user");
        assert_eq!(response.created_at, now);
        assert_eq!(response.updated_at, now);
        assert_eq!(response.token_version, 1);
    }

    #[test]
    fn test_create_user_input_deserialization() {
        let json = r#"{
            "email": "user@example.com",
            "password": "password123",
            "name": "John Doe"
        }"#;

        let input: CreateUserInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.email, "user@example.com");
        assert_eq!(input.password, "password123");
        assert_eq!(input.name, "John Doe");
    }

    #[test]
    fn test_update_user_input_deserialization() {
        let json = r#"{
            "email": "newemail@example.com",
            "name": "Updated Name",
            "role": "admin"
        }"#;

        let input: UpdateUserInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.email, Some("newemail@example.com".to_string()));
        assert_eq!(input.name, Some("Updated Name".to_string()));
        assert_eq!(input.role, Some("admin".to_string()));
    }

    #[test]
    fn test_update_user_input_partial() {
        let json = r#"{
            "name": "Only Name"
        }"#;

        let input: UpdateUserInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.email, None);
        assert_eq!(input.name, Some("Only Name".to_string()));
        assert_eq!(input.role, None);
    }

    #[test]
    fn test_user_response_serialization() {
        let now = Utc::now();
        let response = UserResponse {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            role: "user".to_string(),
            email_verified: false,
            created_at: now,
            updated_at: now,
            token_version: 1,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test@example.com"));
        assert!(json.contains("Test User"));
        assert!(json.contains("user"));
        // token_version is intentionally skipped from external API responses
        assert!(!json.contains("token_version"));
    }
}
