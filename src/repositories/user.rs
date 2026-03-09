use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// User database entity model
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
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
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl From<Model> for UserResponse {
    fn from(model: Model) -> Self {
        Self {
            id: model.id,
            email: model.email,
            name: model.name,
            role: model.role,
            created_at: model.created_at,
            updated_at: model.updated_at,
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
        };
        
        let response = UserResponse::from(model.clone());
        
        assert_eq!(response.id, user_id);
        assert_eq!(response.email, "test@example.com");
        assert_eq!(response.name, "Test User");
        assert_eq!(response.role, "user");
        assert_eq!(response.created_at, now);
        assert_eq!(response.updated_at, now);
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
            created_at: now,
            updated_at: now,
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test@example.com"));
        assert!(json.contains("Test User"));
        assert!(json.contains("user"));
    }
}
