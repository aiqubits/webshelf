use crate::repositories::user::{
    ActiveModel, Column, CreateUserInput, Entity as UserEntity, Model as UserModel,
    UpdateUserInput, UserResponse,
};
use crate::utils::password::{hash_password, verify_password};
use crate::utils::validator::require_password;
use anyhow::Context;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set, Statement, TransactionTrait,
};
use uuid::Uuid;

/// Typed errors for user service operations
#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("User not found")]
    NotFound,
    #[error("Email already registered")]
    EmailConflict,
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Operation forbidden: {0}")]
    Forbidden(String),
    #[error("Weak password: {0}")]
    WeakPassword(String),
    #[error("Password unchanged: {0}")]
    SamePassword(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// User service for CRUD operations
pub struct UserService {
    db: DatabaseConnection,
}
/// Pagination parameters
#[derive(Debug)]
pub struct PaginationParams {
    pub page: u64,
    pub per_page: u64,
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 10,
        }
    }
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
    pub async fn create_user(&self, input: CreateUserInput) -> Result<UserResponse, UserError> {
        tracing::trace!("Creating user with email: {}", input.email);

        let password_hash = hash_password(&input.password).context("Failed to hash password")?;

        let now = Utc::now();
        let user = ActiveModel {
            id: Set(Uuid::new_v4()),
            email: Set(input.email.to_lowercase()),
            password_hash: Set(password_hash),
            name: Set(input.name),
            role: Set("user".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            token_version: Set(1),
            email_verified: Set(false),
            verification_code_hash: Set(None),
            verification_code_expires_at: Set(None),
            verification_code_sent_at: Set(None),
            verification_failed_attempts: Set(0),
        };

        tracing::debug!("Inserting user into database");
        let result = user.insert(&self.db).await.map_err(|e| {
            if matches!(
                e.sql_err(),
                Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
            ) {
                UserError::EmailConflict
            } else {
                UserError::Internal(anyhow::Error::from(e).context("Failed to create user"))
            }
        })?;
        tracing::debug!("User inserted successfully");

        tracing::info!("User created: {}", result.email);
        Ok(UserResponse::from(result))
    }

    /// Get user by ID
    pub async fn get_user(&self, id: Uuid) -> Result<Option<UserResponse>, UserError> {
        let user = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query user")?;

        Ok(user.map(UserResponse::from))
    }

    /// Get user by ID including the password hash (for internal auth flows).
    pub async fn get_user_with_hash(&self, id: Uuid) -> Result<Option<UserModel>, UserError> {
        let user = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query user")?;

        Ok(user)
    }

    /// Get user by email
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserModel>, UserError> {
        let email_normalized = email.to_lowercase();
        let user = UserEntity::find()
            .filter(Column::Email.eq(&email_normalized))
            .one(&self.db)
            .await
            .context("Failed to query user")?;

        Ok(user)
    }

    /// Update user
    pub async fn update_user(
        &self,
        id: Uuid,
        input: UpdateUserInput,
    ) -> Result<UserResponse, UserError> {
        let user = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query user")?
            .ok_or(UserError::NotFound)?;

        // Prevent modification of the system admin account (security boundary)
        if user.role == "system" {
            return Err(UserError::Forbidden(
                "Cannot modify the system admin account".to_string(),
            ));
        }

        let old_role = user.role.clone();
        let mut active_model: ActiveModel = user.into();

        if let Some(email) = input.email {
            active_model.email = Set(email.to_lowercase());
        }
        if let Some(name) = input.name {
            active_model.name = Set(name);
        }

        // Always exclude token_version from the ActiveModel update to avoid
        // overwriting a concurrent atomic increment (password change or role change).
        // When role actually changes, it will be atomically incremented inside
        // the transaction below via raw SQL.
        active_model.token_version = sea_orm::ActiveValue::NotSet;

        let mut token_version_stmt: Option<Statement> = None;

        if let Some(ref new_role) = input.role {
            tracing::info!(
                target_user_id = %id,
                old_role = %old_role,
                new_role = %new_role,
                "Role change requested"
            );
            active_model.role = Set(new_role.clone());

            if *new_role != old_role {
                token_version_stmt = Some(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    "UPDATE users SET token_version = token_version + 1 WHERE id = $1",
                    [id.into()],
                ));
            }
        }

        active_model.updated_at = Set(Utc::now());

        // When the role changed, wrap the token_version increment and the field
        // update in a single transaction so that a partial failure (e.g. email
        // conflict) does not leave token_version incremented while the requested
        // changes are rolled back.
        let result = if let Some(stmt) = token_version_stmt {
            let txn = self
                .db
                .begin()
                .await
                .context("Failed to begin transaction for role change")?;
            txn.execute(stmt)
                .await
                .context("Failed to atomically increment token_version")?;
            let _updated = active_model.update(&txn).await.map_err(|e| {
                if matches!(
                    e.sql_err(),
                    Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
                ) {
                    UserError::EmailConflict
                } else {
                    UserError::Internal(
                        anyhow::Error::from(e).context("Failed to update user in transaction"),
                    )
                }
            })?;
            txn.commit()
                .await
                .context("Failed to commit role-change transaction")?;
            // Re-query to obtain the atomically-incremented token_version
            // (ActiveModel::update returns the model with the old token_version
            //  because it was set to NotSet in the SET clause).
            UserEntity::find_by_id(id)
                .one(&self.db)
                .await
                .context("Failed to re-query user after role change")?
                .ok_or(UserError::NotFound)?
        } else {
            active_model.update(&self.db).await.map_err(|e| {
                if matches!(
                    e.sql_err(),
                    Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
                ) {
                    UserError::EmailConflict
                } else {
                    UserError::Internal(anyhow::Error::from(e).context("Failed to update user"))
                }
            })?
        };

        tracing::info!("User updated: {}", result.email);
        Ok(UserResponse::from(result))
    }

    /// Change user password.
    ///
    /// Verifies the current password, validates new password strength,
    /// hashes the new password, updates the database, and increments
    /// `token_version` to invalidate all existing JWTs.
    ///
    /// Returns the updated `UserResponse` together with the new `token_version`
    /// so the caller can issue a fresh JWT.
    pub async fn change_password(
        &self,
        id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<(UserResponse, i32), UserError> {
        let user = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query user")?
            .ok_or(UserError::NotFound)?;

        // Verify current password
        let is_valid = verify_password(current_password, &user.password_hash)
            .context("Failed to verify password")?;
        if !is_valid {
            return Err(UserError::InvalidCredentials);
        }

        // Reject unchanged password — prevents wasted crypto work and
        // unnecessary token_version increment (defense-in-depth; the handler
        // also checks this, but the service owns the semantic boundary).
        if current_password == new_password {
            return Err(UserError::SamePassword(
                "New password must be different from current password".to_string(),
            ));
        }

        // Validate new password strength
        require_password(new_password).map_err(UserError::WeakPassword)?;

        // Hash new password
        let new_hash = hash_password(new_password).context("Failed to hash password")?;

        // Atomically increment token_version at the database level.
        // The raw SQL "SET token_version = token_version + 1" evaluates the
        // increment using the current DB value, avoiding the read-modify-write
        // race condition that would occur with the application-level pattern
        // "read → compute → write".
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE users SET token_version = token_version + 1 WHERE id = $1",
            [id.into()],
        );

        // Wrap the token_version increment and the password update in a single
        // transaction so that a partial failure does not leave token_version
        // incremented while the password remains unchanged.
        let txn = self
            .db
            .begin()
            .await
            .context("Failed to begin transaction for password change")?;
        txn.execute(stmt)
            .await
            .context("Failed to atomically increment token_version")?;

        // Update password_hash and updated_at via ActiveModel.
        // token_version is explicitly set to NotSet so the ActiveModel update
        // does not overwrite the atomically-incremented value.
        let mut active_model: ActiveModel = user.into();
        active_model.token_version = sea_orm::ActiveValue::NotSet;
        active_model.password_hash = Set(new_hash);
        active_model.updated_at = Set(Utc::now());

        active_model.update(&txn).await.map_err(|e| {
            UserError::Internal(anyhow::Error::from(e).context("Failed to update password"))
        })?;

        txn.commit()
            .await
            .context("Failed to commit password-change transaction")?;

        tracing::info!("User {} changed password", id);

        // Re-query to obtain the atomically-incremented token_version.
        let updated = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to re-query user after password change")?
            .ok_or(UserError::NotFound)?;
        let new_version = updated.token_version;
        Ok((UserResponse::from(updated), new_version))
    }

    /// Delete user
    pub async fn delete_user(&self, id: Uuid) -> Result<(), UserError> {
        // Prevent deletion of the system admin account
        let target = UserEntity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query user")?
            .ok_or(UserError::NotFound)?;

        if target.role == "system" {
            return Err(UserError::Forbidden(
                "Cannot delete the system admin account".to_string(),
            ));
        }

        let result = UserEntity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("Failed to delete user")?;

        if result.rows_affected == 0 {
            return Err(UserError::NotFound);
        }

        tracing::info!("User deleted: {}", id);
        Ok(())
    }

    /// List users with pagination
    pub async fn list_users(
        &self,
        params: PaginationParams,
    ) -> Result<PaginatedResponse<UserResponse>, UserError> {
        // Sanitize pagination inputs: clamp zero to 1 and cap large values
        // to reasonable bounds to prevent overflow or excessive offsets.
        let page = params.page.clamp(1, 1_000_000);
        let per_page = params.per_page.clamp(1, 100);

        let paginator = UserEntity::find()
            .order_by_desc(Column::CreatedAt)
            .paginate(&self.db, per_page);

        let total = paginator
            .num_items()
            .await
            .context("Failed to count users")?;
        let total_pages = paginator
            .num_pages()
            .await
            .context("Failed to count pages")?;

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
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 10);
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
        use crate::repositories::user::UserResponse;
        use chrono::Utc;
        use uuid::Uuid;

        let user = UserResponse {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            role: "user".to_string(),
            email_verified: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            token_version: 1,
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
        assert_eq!(
            response.total_pages,
            response.total.div_ceil(response.per_page)
        );
    }
}
