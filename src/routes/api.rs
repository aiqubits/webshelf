use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::models::user::{CreateUserInput, UpdateUserInput, UserResponse};
use crate::services::user::{PaginatedResponse, PaginationParams, UserService};
use crate::utils::error::ApiError;
use crate::AppState;

/// Create API routes
pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Health check endpoint (public)
        .route("/health", get(health_check))
        // User management endpoints
        .route("/users", get(list_users))
        .route("/users", post(create_user))
        .route("/users/:id", get(get_user))
        .route("/users/:id", put(update_user))
        .route("/users/:id", delete(delete_user))
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Query parameters for listing users
#[derive(Debug, Deserialize)]
struct ListUsersQuery {
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
struct PaginatedUsersResponse {
    items: Vec<UserResponse>,
    total: u64,
    page: u64,
    per_page: u64,
    total_pages: u64,
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
async fn list_users(
    State(state): State<AppState>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<PaginatedUsersResponse>, ApiError> {
    let service = UserService::new(state.db.clone());
    let result = service
        .list_users(PaginationParams {
            page: query.page,
            per_page: query.per_page,
        })
        .await?;

    Ok(Json(PaginatedUsersResponse::from(result)))
}

/// Create user request with validation
#[derive(Debug, Deserialize, Validate)]
struct CreateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    password: String,

    #[validate(length(min = 2, max = 50, message = "name must be between 2 and 50 characters"))]
    name: String,
}

/// Create a new user
async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Validate request payload
    payload.validate()?;

    let service = UserService::new(state.db.clone());
    let result = service
        .create_user(CreateUserInput {
            email: payload.email,
            password: payload.password,
            name: payload.name,
        })
        .await?;

    Ok(Json(result))
}

/// Get a user by ID
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserResponse>, ApiError> {
    let service = UserService::new(state.db.clone());
    let result = service
        .get_user(id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    Ok(Json(result))
}

/// Update user request with validation
#[derive(Debug, Deserialize, Validate)]
struct UpdateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: Option<String>,

    #[validate(length(min = 2, max = 50, message = "name must be between 2 and 50 characters"))]
    name: Option<String>,

    role: Option<String>,
}

/// Update a user
async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
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

    Ok(Json(result))
}

/// Delete a user
async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = UserService::new(state.db.clone());
    service.delete_user(id).await?;

    Ok(Json(serde_json::json!({
        "message": "User deleted successfully"
    })))
}
