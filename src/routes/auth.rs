use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::services::auth::{AuthService, LoginRequest, LoginResponse};
use crate::utils::error::ApiError;
use crate::AppState;

/// Create authentication routes
pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
}

/// Login request with validation
#[derive(Debug, Deserialize, Validate)]
struct LoginRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 1, message = "password is required"))]
    password: String,
}

/// Login endpoint
async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequestBody>,
) -> Result<Json<LoginResponse>, ApiError> {
    // Validate request
    payload.validate()?;

    let service = AuthService::new(
        state.db.clone(),
        state.config.jwt_secret.clone(),
        state.config.jwt_expiry_seconds,
    );

    let result = service
        .login(LoginRequest {
            email: payload.email,
            password: payload.password,
        })
        .await?;

    Ok(Json(result))
}

/// Register request with validation
#[derive(Debug, Deserialize, Validate)]
struct RegisterRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    password: String,

    #[validate(length(min = 2, max = 50, message = "name must be between 2 and 50 characters"))]
    name: String,
}

/// Register response
#[derive(Serialize)]
struct RegisterResponse {
    message: String,
    user_id: String,
}

/// Register endpoint
async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequestBody>,
) -> Result<Json<RegisterResponse>, ApiError> {
    // Validate request
    payload.validate()?;

    use crate::models::user::CreateUserInput;
    use crate::services::user::UserService;

    let service = UserService::new(state.db.clone());
    let user = service
        .create_user(CreateUserInput {
            email: payload.email,
            password: payload.password,
            name: payload.name,
        })
        .await?;

    Ok(Json(RegisterResponse {
        message: "User registered successfully".to_string(),
        user_id: user.id.to_string(),
    }))
}
