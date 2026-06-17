use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::AppState;
use crate::middlewares::auth::AuthUser;
use crate::repositories::user::{CreateUserInput, UpdateUserInput, UserResponse};
use crate::services::user::{PaginatedResponse, PaginationParams, UserService};
use crate::services::verification::{VerificationError, VerificationService};
use crate::utils::JsonOrForm;
use crate::utils::error::ApiError;
use crate::utils::validator::check_password_strength;

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    version: String,
}

/// Health check endpoint
pub async fn health_check() -> Json<HealthResponse> {
    tracing::trace!("Health check endpoint called");
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
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
    Extension(auth_user): Extension<AuthUser>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<PaginatedUsersResponse>, ApiError> {
    let service = UserService::new(state.db.clone());
    let result = service
        .list_users(
            PaginationParams {
                page: query.page,
                per_page: query.per_page,
            },
            &auth_user.role,
        )
        .await?;

    Ok(Json(PaginatedUsersResponse::from(result)))
}

/// Create user request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    password: String,

    #[validate(length(
        min = 2,
        max = 50,
        message = "name must be between 2 and 50 characters"
    ))]
    name: String,

    /// Role override (only effective when actor is system)
    #[validate(custom(function = "validate_role"))]
    role: Option<String>,
}

/// Create a new user
///
/// After creation, handles email verification consistent with the public
/// registration flow: if the email service is configured, sends a verification
/// email; if not, auto-verifies the user so admin-created accounts can log in.
pub async fn create_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    JsonOrForm(payload): JsonOrForm<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Validate request payload
    payload.validate()?;

    // Validate password strength (complexity rules)
    check_password_strength(&payload.password)?;

    // Normalize email to lowercase once at the entry point
    let email = payload.email.to_lowercase();

    // Non-system actors cannot assign roles via create; only system can.
    // Normalize to None early so the service layer is not sent a role
    // request that would be ignored anyway.
    let requested_role = if auth_user.role == "system" {
        payload.role
    } else {
        None
    };

    let service = UserService::new(state.db.clone());
    let mut result = service
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: payload.password,
                name: payload.name,
                role: requested_role,
            },
            &auth_user.role,
        )
        .await?;

    // Handle email verification (consistent with public registration flow).
    // Admin-created users should be able to log in — if email service is
    // not configured, auto-verify; if send fails, auto-verify as fallback.
    let verification = VerificationService::new(state.db.clone(), state.email.clone());
    match verification.send_verification_email(&email).await {
        Ok(()) => {
            tracing::info!("Verification code sent to admin-created user: {}", email);
        }
        Err(err) => {
            // Log the specific reason for the failure
            match &err {
                VerificationError::EmailNotConfigured => {
                    tracing::warn!(
                        "Email service not configured — auto-verifying admin-created user: {}",
                        email
                    );
                }
                _ => {
                    tracing::error!("Failed to send verification email: {:?}", err);
                }
            }
            // Fallback: auto-verify so admin-created users can log in.
            // This handles both the case where email is not configured
            // (dev/test) and transient SMTP failures.
            if let Err(e) = verification.auto_verify(&email).await {
                tracing::error!(
                    "Failed to auto-verify admin-created user after email send failure: {:?}",
                    e
                );
                return Err(ApiError::Internal(
                    "An unexpected error occurred".to_string(),
                ));
            }
            result.email_verified = true;
        }
    }

    Ok(Json(result))
}

/// Get current user profile — `GET /api/users/me` (any authenticated user)
pub async fn get_me(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<UserResponse>, ApiError> {
    let user_id: i64 = auth_user.user_id.parse().map_err(|_| {
        tracing::error!("Invalid user ID in auth token: {}", auth_user.user_id);
        ApiError::Internal("An unexpected error occurred".to_string())
    })?;

    let service = UserService::new(state.db.clone());
    let result = service
        .get_user(user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    Ok(Json(result))
}

/// Change password request body
#[derive(Debug, Deserialize, Validate)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 1, message = "current password is required"))]
    pub current_password: String,
    #[validate(length(min = 1, message = "new password is required"))]
    pub new_password: String,
}

/// Change password response
#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub message: String,
    pub new_token: String,
}

/// Change current user's password — `POST /api/users/me/password` (any authenticated user)
///
/// Flow: validate input → delegate to UserService (verify, hash, update, bump token_version)
/// → issue fresh JWT.
pub async fn change_my_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    JsonOrForm(payload): JsonOrForm<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, ApiError> {
    payload.validate()?;

    check_password_strength(&payload.new_password)?;

    let user_id: i64 = auth_user.user_id.parse().map_err(|_| {
        tracing::error!("Invalid user ID in auth token: {}", auth_user.user_id);
        ApiError::Internal("An unexpected error occurred".to_string())
    })?;

    if payload.current_password == payload.new_password {
        return Err(ApiError::BadRequest(
            "New password must be different from current password".to_string(),
        ));
    }

    let service = UserService::new(state.db.clone());
    let (user, token_version) = service
        .change_password(user_id, &payload.current_password, &payload.new_password)
        .await?;

    // Issue a fresh JWT (the old one is now invalid due to token_version increment).
    // Use the user's data from the database rather than from the old
    // JWT (auth_user) — the database is the authoritative source of truth.
    let new_token = crate::middlewares::auth::generate_token(
        &user.id.to_string(),
        &user.role,
        &state.config.jwt_secret,
        state.config.jwt_expiry_seconds,
        token_version,
    )
    .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;

    Ok(Json(ChangePasswordResponse {
        message: "Password changed successfully".to_string(),
        new_token,
    }))
}

/// Get a user by ID
pub async fn get_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<Json<UserResponse>, ApiError> {
    let service = UserService::new(state.db.clone());
    let result = service
        .get_user_scoped(id, &auth_user.role)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    Ok(Json(result))
}

/// Update user request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: Option<String>,

    #[validate(length(
        min = 2,
        max = 50,
        message = "name must be between 2 and 50 characters"
    ))]
    name: Option<String>,

    #[validate(custom(function = "validate_role"))]
    role: Option<String>,
}

/// Validate role value against allowed roles.
/// "system" is intentionally excluded — it can only be set during bootstrap seeding,
/// never via the admin API, to prevent privilege escalation.
fn validate_role(role: &str) -> Result<(), ValidationError> {
    match role {
        "user" | "admin" => Ok(()),
        _ => {
            let mut err = validator::ValidationError::new("invalid_role");
            err.message = Some("role must be 'user' or 'admin'".into());
            Err(err)
        }
    }
}

/// Update a user
pub async fn update_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
    JsonOrForm(payload): JsonOrForm<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Validate request payload
    payload.validate()?;

    // Require at least one field to be provided
    if payload.email.is_none() && payload.name.is_none() && payload.role.is_none() {
        return Err(ApiError::BadRequest(
            "At least one field (email, name, or role) must be provided".to_string(),
        ));
    }

    // Non-system actors cannot modify roles via update; only system can.
    // Normalize to None early so the service layer is not sent a role
    // request that would be ignored anyway.
    let requested_role = if auth_user.role == "system" {
        payload.role
    } else {
        None
    };

    let service = UserService::new(state.db.clone());
    let result = service
        .update_user(
            id,
            UpdateUserInput {
                email: payload.email,
                name: payload.name,
                role: requested_role,
            },
            &auth_user.role,
        )
        .await?;

    Ok(Json(result))
}

/// Delete a user
pub async fn delete_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let actor_id: i64 = auth_user.user_id.parse().map_err(|_| {
        tracing::error!("Invalid user ID in auth token: {}", auth_user.user_id);
        ApiError::Internal("An unexpected error occurred".to_string())
    })?;
    let service = UserService::new(state.db.clone());
    service.delete_user(id, &auth_user.role, actor_id).await?;

    Ok(Json(serde_json::json!({
        "message": "User deleted successfully"
    })))
}
