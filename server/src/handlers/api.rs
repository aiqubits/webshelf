use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use sea_orm::{ActiveModelTrait, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::middlewares::auth::AuthUser;
use crate::repositories::user::{CreateUserInput, UpdateUserInput, UserResponse};
use crate::services::user::{PaginatedResponse, PaginationParams, UserService};
use crate::utils::error::ApiError;
use crate::utils::password::verify_password;
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
}

/// Create a new user
pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Validate request payload
    payload.validate()?;

    // Validate password strength (complexity rules)
    check_password_strength(&payload.password)?;

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

/// Get current user profile — `GET /api/users/me`（任意已认证用户）
pub async fn get_me(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<UserResponse>, ApiError> {
    let user_id = uuid::Uuid::parse_str(&auth_user.user_id)
        .map_err(|_| ApiError::Internal("Invalid user ID in token".to_string()))?;

    let service = UserService::new(state.db.clone());
    let result = service
        .get_user(user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    Ok(Json(result))
}

/// Change password request body
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    /// 当前密码（用于验证身份）
    pub current_password: String,
    /// 新密码
    pub new_password: String,
}

/// Change password response
#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub message: String,
}

/// Change current user's password — `POST /api/users/me/password`（任意已认证用户）
///
/// 流程：验证当前密码 → 校验新密码强度 → 哈希新密码 → 更新数据库。
pub async fn change_my_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, ApiError> {
    let user_id = Uuid::parse_str(&auth_user.user_id)
        .map_err(|_| ApiError::Internal("Invalid user ID in token".to_string()))?;

    if payload.current_password.is_empty() {
        return Err(ApiError::BadRequest("当前密码不能为空".to_string()));
    }
    if payload.new_password.is_empty() {
        return Err(ApiError::BadRequest("新密码不能为空".to_string()));
    }
    if payload.current_password == payload.new_password {
        return Err(ApiError::BadRequest("新密码不能与当前密码相同".to_string()));
    }

    // 1. 查询用户实体（需要 password_hash 来验证当前密码）
    let service = UserService::new(state.db.clone());
    let user = service
        .get_user_with_hash(user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    // 2. 验证当前密码
    let is_valid = verify_password(&payload.current_password, &user.password_hash)
        .map_err(|e| ApiError::Internal(format!("Password verification failed: {}", e)))?;
    if !is_valid {
        return Err(ApiError::Unauthorized("当前密码错误".to_string()));
    }

    // 3. 校验新密码强度
    check_password_strength(&payload.new_password)?;

    // 4. 哈希并更新
    let new_hash = crate::utils::password::hash_password(&payload.new_password)
        .map_err(|e| ApiError::Internal(format!("Failed to hash password: {}", e)))?;

    let now = chrono::Utc::now();
    let mut active_model: crate::repositories::user::ActiveModel = user.into();
    active_model.password_hash = Set(new_hash);
    active_model.updated_at = Set(now);
    active_model.update(&state.db).await?;

    tracing::info!("User {} changed password", auth_user.user_id);

    Ok(Json(ChangePasswordResponse {
        message: "密码修改成功".to_string(),
    }))
}

/// Get a user by ID
pub async fn get_user(
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

/// Validate role value against allowed roles
fn validate_role(role: &str) -> Result<(), validator::ValidationError> {
    match role {
        "user" | "admin" | "system" => Ok(()),
        _ => {
            let mut err = validator::ValidationError::new("invalid_role");
            err.message = Some("role must be 'user', 'admin', or 'system'".into());
            Err(err)
        }
    }
}

/// Update a user
pub async fn update_user(
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
pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = UserService::new(state.db.clone());
    service.delete_user(id).await?;

    Ok(Json(serde_json::json!({
        "message": "User deleted successfully"
    })))
}
