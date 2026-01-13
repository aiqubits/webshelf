use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::models::user::{CreateUserInput, UpdateUserInput, UserResponse};
use crate::services::user::{PaginatedResponse, PaginationParams, UserService};
use crate::utils::error::ApiError;
use crate::AppState;
use crate::utils::common::{*};

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    version: String,
}

/// Health check endpoint
pub async fn health_check() -> R<()> {
    tracing::info!("Health check endpoint called");
    AppResult::ok(()).into()
}

/// Query parameters for listing users
#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    #[serde(default = "default_page")]
    page: u64,
    #[serde(default = "default_per_page")]
    per_page: u64,
}

fn default_page() -> u64 {
    1
}

fn default_per_page() -> u64 {
    10
}

/// Paginated users response
#[derive(Serialize)]
pub struct PaginatedUsersResponse {
    pub items: Vec<UserResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

impl From<PaginatedResponse<UserResponse>> for PaginatedUsersResponse {
    fn from(resp: PaginatedResponse<UserResponse>) -> Self {
        Self {
            items: resp.items,
            total: resp.total,
            page: resp.page,
            per_page: resp.per_page,
            total_pages: resp.total_pages,
        }
    }
}

/// List users with pagination
pub async fn list_users(
    State(state): State<AppState>,
    Query(query): Query<ListUsersQuery>,
) -> R<PaginatedUsersResponse> {
    let service = UserService::new(state.db.clone());
    let result = service
        .list_users(PaginationParams {
            page: query.page,
            per_page: query.per_page,
        })
        .await?;

    AppResult::ok(PaginatedUsersResponse::from(result)).into()
}

/// Create user request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    password: String,

    #[validate(length(min = 2, max = 50, message = "name must be between 2 and 50 characters"))]
    name: String,
}

/// Create a new user
pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> R<UserResponse> {
    // Validate request payload
    payload.validate()?;

    let service = UserService::new(state.db.clone());
    let result = service
        .create_user(CreateUserInput {
            email: payload.email,
            password: payload.password,
            name: payload.name,
        })
        .await
        .map_err(|e| {
            let error_msg = e.to_string();
            tracing::error!("Failed to create user: {}", error_msg);
            
            // Check if it's an email conflict error
            if error_msg.contains("Email already registered") {
                ApiError::Conflict(error_msg.replace("Internal server error: ", ""))
            } else {
                ApiError::Internal(error_msg)
            }
        })?;

    AppResult::ok(result).into()
}

/// Get a user by ID
pub async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> R<UserResponse> {
    let service = UserService::new(state.db.clone());
    let result = service
        .get_user(id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    AppResult::ok(result).into()
}

/// Update user request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: Option<String>,

    #[validate(length(min = 2, max = 50, message = "name must be between 2 and 50 characters"))]
    name: Option<String>,

    role: Option<String>,
}

/// Update a user
pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateUserRequest>,
) -> R<UserResponse> {
    // Validate request payload
    payload.validate()?;

    let service = UserService::new(state.db.clone());
    let result = service
        .update_user(
            id,
            UpdateUserInput {
                email: payload.email,
                name: payload.name,
                role: payload.role,
            },
        )
        .await?;

    AppResult::ok(result).into()
}

/// Delete a user
pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> R<()>{
    let service = UserService::new(state.db.clone());
    service.delete_user(id).await?;

    AppResult::ok(()).into()
}
