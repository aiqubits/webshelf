use crate::models::user::{
    ActiveModel, Column, CreateUserInput, Entity as UserEntity, Model as UserModel,
    UpdateUserInput, UserResponse,
};
use crate::utils::password::hash_password;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set,
};
use uuid::Uuid;

/// User service for CRUD operations
pub struct UserService {
    db: DatabaseConnection,
}

/// Pagination parameters
#[derive(Debug, Default)]
pub struct PaginationParams {
    pub page: u64,
    pub per_page: u64,
}

/// Paginated response
#[derive(Debug)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

impl UserService {
    /// Create a new user service
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Create a new user
    pub async fn create_user(&self, input: CreateUserInput) -> Result<UserResponse> {
        // Check if email already exists
        let existing = UserEntity::find()
            .filter(Column::Email.eq(&input.email))
            .one(&self.db)
            .await
            .context("Failed to check existing user")?;

        if existing.is_some() {
            return Err(anyhow!("Email already registered"));
        }

        // Hash password
        let password_hash = hash_password(&input.password).context("Failed to hash password")?;

        let now = Utc::now();
        let user = ActiveModel {
            id: Set(Uuid::new_v4()),
            email: Set(input.email),
            password_hash: Set(password_hash),
            name: Set(input.name),
            role: Set("user".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        };

        let result = user
            .insert(&self.db)
            .await
            .context("Failed to create user")?;

        tracing::info!("User created: {}", result.email);
        Ok(UserResponse::from(result))
    }

    /// Get user by ID
    pub async fn get_user(&self, id: Uuid) -> Result<Option<UserResponse>> {
        let user = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query user")?;

        Ok(user.map(UserResponse::from))
    }

    /// Get user by email
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserModel>> {
        let user = UserEntity::find()
            .filter(Column::Email.eq(email))
            .one(&self.db)
            .await
            .context("Failed to query user")?;

        Ok(user)
    }

    /// Update user
    pub async fn update_user(&self, id: Uuid, input: UpdateUserInput) -> Result<UserResponse> {
        let user = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query user")?
            .ok_or_else(|| anyhow!("User not found"))?;

        let mut active_model: ActiveModel = user.into();

        if let Some(email) = input.email {
            active_model.email = Set(email);
        }
        if let Some(name) = input.name {
            active_model.name = Set(name);
        }
        if let Some(role) = input.role {
            active_model.role = Set(role);
        }
        active_model.updated_at = Set(Utc::now());

        let result = active_model
            .update(&self.db)
            .await
            .context("Failed to update user")?;

        tracing::info!("User updated: {}", result.email);
        Ok(UserResponse::from(result))
    }

    /// Delete user
    pub async fn delete_user(&self, id: Uuid) -> Result<()> {
        let result = UserEntity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("Failed to delete user")?;

        if result.rows_affected == 0 {
            return Err(anyhow!("User not found"));
        }

        tracing::info!("User deleted: {}", id);
        Ok(())
    }

    /// List users with pagination
    pub async fn list_users(
        &self,
        params: PaginationParams,
    ) -> Result<PaginatedResponse<UserResponse>> {
        let page = if params.page == 0 { 1 } else { params.page };
        let per_page = if params.per_page == 0 {
            10
        } else {
            params.per_page.min(100)
        };

        let paginator = UserEntity::find()
            .order_by_desc(Column::CreatedAt)
            .paginate(&self.db, per_page);

        let total = paginator.num_items().await.context("Failed to count users")?;
        let total_pages = paginator.num_pages().await.context("Failed to count pages")?;

        let users = paginator
            .fetch_page(page - 1)
            .await
            .context("Failed to fetch users")?;

        Ok(PaginatedResponse {
            items: users.into_iter().map(UserResponse::from).collect(),
            total,
            page,
            per_page,
            total_pages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_params_default() {
        let params = PaginationParams::default();
        assert_eq!(params.page, 0);
        assert_eq!(params.per_page, 0);
    }

    #[test]
    fn test_pagination_params_custom() {
        let params = PaginationParams {
            page: 2,
            per_page: 20,
        };
        assert_eq!(params.page, 2);
        assert_eq!(params.per_page, 20);
    }

    #[test]
    fn test_paginated_response_structure() {
        let response: PaginatedResponse<UserResponse> = PaginatedResponse {
            items: vec![],
            total: 100,
            page: 1,
            per_page: 10,
            total_pages: 10,
        };
        
        assert_eq!(response.total, 100);
        assert_eq!(response.page, 1);
        assert_eq!(response.per_page, 10);
        assert_eq!(response.total_pages, 10);
        assert_eq!(response.items.len(), 0);
    }

    #[test]
    fn test_pagination_params_debug() {
        let params = PaginationParams {
            page: 3,
            per_page: 15,
        };
        let debug_str = format!("{:?}", params);
        assert!(debug_str.contains("PaginationParams"));
        assert!(debug_str.contains("3"));
        assert!(debug_str.contains("15"));
    }

    #[test]
    fn test_paginated_response_debug() {
        let response: PaginatedResponse<String> = PaginatedResponse {
            items: vec!["item1".to_string()],
            total: 50,
            page: 2,
            per_page: 25,
            total_pages: 2,
        };
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("PaginatedResponse"));
        assert!(debug_str.contains("50"));
    }

    #[test]
    fn test_paginated_response_with_items() {
        use crate::models::user::UserResponse;
        use chrono::Utc;
        use uuid::Uuid;

        let user = UserResponse {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            role: "user".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let response = PaginatedResponse {
            items: vec![user],
            total: 1,
            page: 1,
            per_page: 10,
            total_pages: 1,
        };

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].email, "test@example.com");
    }

    #[test]
    fn test_paginated_response_generic_string() {
        let response: PaginatedResponse<String> = PaginatedResponse {
            items: vec!["a".to_string(), "b".to_string()],
            total: 2,
            page: 1,
            per_page: 10,
            total_pages: 1,
        };
        
        assert_eq!(response.items.len(), 2);
        assert_eq!(response.items[0], "a");
        assert_eq!(response.items[1], "b");
    }

    #[test]
    fn test_paginated_response_generic_integer() {
        let response: PaginatedResponse<i32> = PaginatedResponse {
            items: vec![1, 2, 3],
            total: 3,
            page: 1,
            per_page: 10,
            total_pages: 1,
        };
        
        assert_eq!(response.items.len(), 3);
        assert_eq!(response.total, 3);
    }

    #[test]
    fn test_pagination_boundary_values() {
        let params = PaginationParams {
            page: u64::MAX,
            per_page: u64::MAX,
        };
        assert_eq!(params.page, u64::MAX);
        assert_eq!(params.per_page, u64::MAX);
    }

    #[test]
    fn test_paginated_response_empty() {
        let response: PaginatedResponse<UserResponse> = PaginatedResponse {
            items: vec![],
            total: 0,
            page: 1,
            per_page: 10,
            total_pages: 0,
        };
        
        assert!(response.items.is_empty());
        assert_eq!(response.total, 0);
        assert_eq!(response.total_pages, 0);
    }

    #[test]
    fn test_paginated_response_calculation() {
        let response: PaginatedResponse<i32> = PaginatedResponse {
            items: vec![],
            total: 95,
            page: 10,
            per_page: 10,
            total_pages: 10,
        };
        
        // Verify pagination math: 95 items / 10 per_page = 10 pages
        assert_eq!(response.total_pages, (response.total + response.per_page - 1) / response.per_page);
    }
}
