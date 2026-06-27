use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::AppState;
use crate::handlers::auth::{expiry_cookie, token_cookie, unix_timestamp_from_now};
use crate::handlers::helpers::extract_handler_context;
use crate::middlewares::{AuthUser, JWT_COOKIE, REFRESH_COOKIE};
use crate::repositories::user::{CreateUserInput, UpdateUserInput, UserResponse};
use crate::services::auth::AuthService;
use crate::services::user::{BALANCE_SCALE, PaginatedResponse, PaginationParams, UserService};
use crate::services::verification::VerificationService;
use crate::utils::error::ApiError;
use crate::utils::validator::check_password_strength;
use webshelf_runtime::{HttpError, RequestContext, Response};

/// Helper: convert through ApiError to HttpError
fn to_http<E: Into<ApiError>>(e: E) -> HttpError {
    let api: ApiError = e.into();
    HttpError::from(api)
}

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    version: String,
}

/// Health check endpoint
pub async fn health_check(_req: crate::ServerRequest) -> Result<Response, HttpError> {
    tracing::trace!("Health check endpoint called");
    Response::json(&HealthResponse {
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
pub async fn list_users(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let query: ListUsersQuery = req.parse_query().map_err(HttpError::bad_request)?;

    let service = UserService::new(state.db.clone(), state.cache.clone());
    let result = service
        .list_users(
            PaginationParams {
                page: query.page,
                per_page: query.per_page,
            },
            &auth_user.role,
        )
        .await
        .map_err(to_http)?;

    Response::json(&PaginatedUsersResponse::from(result))
}

/// Create user request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    password: String,

    #[validate(length(
        min = 6,
        max = 50,
        message = "name must be between 6 and 50 characters"
    ))]
    name: String,

    /// Role override (only effective when actor is system)
    #[validate(custom(function = "validate_role"))]
    role: Option<String>,
}

/// Create a new user (admin-only).
///
/// The created user is **auto-verified** — no verification email is sent and
/// `email_verified` is set to `true` immediately.  This is intentional:
/// admin-created accounts are trusted and should be usable right away.
///
/// If the auto-verify DB update fails the user is still created (though
/// unverified); the request succeeds with `email_verified: false` so the
/// client is not left guessing whether the user exists.
pub async fn create_user(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let payload: CreateUserRequest = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    // Validate request payload
    payload
        .validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    // Validate password strength (complexity rules)
    check_password_strength(&payload.password).map_err(to_http)?;

    // Normalize email to lowercase once at the entry point
    let email = payload.email.to_lowercase();

    let requested_role = if auth_user.role == "system" {
        payload.role
    } else {
        None
    };

    let service = UserService::new(state.db.clone(), state.cache.clone());
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
        .await
        .map_err(to_http)?;

    // Auto-verify the admin-created user.  This is best-effort: if the DB
    // update fails the user was already created, so we log a warning and
    // return the response with `email_verified: false` rather than returning
    // a 500 that leaves the client guessing.
    let verification = VerificationService::new(state.db.clone(), state.email.clone());
    match verification.auto_verify(&email).await {
        Ok(()) => {
            result.email_verified = true;
            tracing::info!(
                "Admin-created user {} (email: {}) is auto-verified",
                result.id,
                email
            );
        }
        Err(e) => {
            tracing::warn!(
                "Admin-created user {} created but auto-verify failed: {:?}. \
                 email_verified=false in DB.",
                result.id,
                e
            );
        }
    }

    Response::json(&result)
}

/// Get current user profile — `GET /api/users/me` (any authenticated user)
pub async fn get_me(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;

    let user_id: i64 = auth_user.user_id.parse().map_err(|_| {
        tracing::error!("Invalid user ID in auth token: {}", auth_user.user_id);
        HttpError::internal("An unexpected error occurred")
    })?;

    let service = UserService::new(state.db.clone(), state.cache.clone());
    let result = service
        .get_user(user_id)
        .await
        .map_err(to_http)?
        .ok_or_else(|| HttpError::not_found("User not found"))?;

    Response::json(&result)
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
pub async fn change_my_password(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let payload: ChangePasswordRequest = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    let result = change_my_password_inner(&state, &auth_user, &payload)
        .await
        .map_err(to_http)?;
    let (resp, cookies) = result;

    let mut response = Response::json(&resp)?;
    for cookie in cookies {
        response.set_cookie(cookie);
    }
    Ok(response)
}

async fn change_my_password_inner(
    state: &AppState,
    auth_user: &AuthUser,
    payload: &ChangePasswordRequest,
) -> Result<(ChangePasswordResponse, Vec<cookie::Cookie<'static>>), ApiError> {
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

    let service = UserService::new(state.db.clone(), state.cache.clone());
    let (user, token_version) = service
        .change_password(user_id, &payload.current_password, &payload.new_password)
        .await?;

    let is_remember = auth_user.remember;
    let jwt_expiry = if is_remember {
        state.config.jwt_remember_expiry_seconds
    } else {
        state.config.jwt_expiry_seconds
    };

    let new_token = crate::middlewares::generate_token(
        &user.id.to_string(),
        &user.role,
        &state.config.jwt_secret,
        jwt_expiry,
        auth_user.remember,
        token_version,
    )
    .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;

    let jwt_max_age = jwt_expiry;
    let jwt_expires_at_unix = unix_timestamp_from_now(jwt_max_age)?;

    let refresh_cookie = token_cookie(REFRESH_COOKIE, "", 0, state.config.cookie_secure);

    let cookies = vec![
        token_cookie(
            JWT_COOKIE,
            &new_token,
            jwt_max_age,
            state.config.cookie_secure,
        ),
        refresh_cookie,
        expiry_cookie(
            &jwt_expires_at_unix.to_string(),
            jwt_max_age,
            state.config.cookie_secure,
        ),
    ];

    Ok((
        ChangePasswordResponse {
            message: "Password changed successfully".to_string(),
            new_token,
        },
        cookies,
    ))
}

/// Logout-all response
#[derive(Serialize)]
pub struct LogoutAllResponse {
    pub message: String,
}

/// Logout from all devices — `POST /api/users/me/logout-all`.
pub async fn logout_all(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;

    let user_id: i64 = auth_user
        .user_id
        .parse()
        .map_err(|_| HttpError::internal("An unexpected error occurred"))?;

    let service = AuthService::new(
        state.db.clone(),
        state.config.jwt_secret.clone(),
        state.config.jwt_expiry_seconds,
        state.config.jwt_remember_expiry_seconds,
        state.config.refresh_token_expiry_seconds,
    );

    service
        .revoke_all_sessions(user_id)
        .await
        .map_err(|_| HttpError::internal("Failed to revoke all sessions"))?;

    let token_cache_key = format!("user:token_version:{}", user_id);
    if let Err(e) = state.cache.invalidate(&token_cache_key).await {
        tracing::warn!(
            "Failed to invalidate token_version cache for user {}: {:?}",
            user_id,
            e
        );
    }

    let cookies = crate::handlers::auth::clear_auth_cookies(state.config.cookie_secure);

    let mut response = Response::json(&LogoutAllResponse {
        message: "Logged out from all devices".to_string(),
    })?;

    for cookie in cookies {
        response.set_cookie(cookie);
    }

    tracing::info!("User {} logged out from all devices", user_id);
    Ok(response)
}

/// Get a user by ID
pub async fn get_user(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let id: i64 = req
        .parse_param("id")
        .map_err(|_| HttpError::bad_request("Invalid or missing user ID"))?;

    let service = UserService::new(state.db.clone(), state.cache.clone());
    let result = service
        .get_user_scoped(id, &auth_user.role)
        .await
        .map_err(to_http)?
        .ok_or_else(|| HttpError::not_found("User not found"))?;

    Response::json(&result)
}

/// Update user request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserRequest {
    #[validate(email(message = "must be a valid email address"))]
    email: Option<String>,

    #[validate(length(
        min = 6,
        max = 50,
        message = "name must be between 6 and 50 characters"
    ))]
    name: Option<String>,

    #[validate(custom(function = "validate_role"))]
    role: Option<String>,
}

/// Validate role value against allowed roles.
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
pub async fn update_user(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let id: i64 = req
        .parse_param("id")
        .map_err(|_| HttpError::bad_request("Invalid or missing user ID"))?;
    let payload: UpdateUserRequest = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    payload
        .validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    if payload.email.is_none() && payload.name.is_none() && payload.role.is_none() {
        return Err(HttpError::bad_request(
            "At least one field (email, name, or role) must be provided",
        ));
    }

    let requested_role = if auth_user.role == "system" {
        payload.role
    } else {
        None
    };

    let service = UserService::new(state.db.clone(), state.cache.clone());
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
        .await
        .map_err(to_http)?;

    Response::json(&result)
}

/// Delete user response
#[derive(Serialize)]
pub struct DeleteUserResponse {
    pub message: String,
}

/// Delete a user
pub async fn delete_user(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let id: i64 = req
        .parse_param("id")
        .map_err(|_| HttpError::bad_request("Invalid or missing user ID"))?;

    let actor_id: i64 = auth_user.user_id.parse().map_err(|_| {
        tracing::error!("Invalid user ID in auth token: {}", auth_user.user_id);
        HttpError::internal("An unexpected error occurred")
    })?;

    let service = UserService::new(state.db.clone(), state.cache.clone());
    service
        .delete_user(id, &auth_user.role, actor_id)
        .await
        .map_err(to_http)?;

    Response::json(&DeleteUserResponse {
        message: "User deleted successfully".to_string(),
    })
}

/// Set balance request body
#[derive(Debug, Deserialize)]
pub struct SetBalanceRequest {
    pub balance: i64,
}

/// Set balance response
#[derive(Serialize)]
pub struct SetBalanceResponse {
    pub balance: i64,
    pub display_balance: f64,
    pub message: String,
}

/// Set a user's balance — `PUT /api/users/{id}/balance` (admin/system only).
pub async fn set_balance(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let id: i64 = req
        .parse_param("id")
        .map_err(|_| HttpError::bad_request("Invalid or missing user ID"))?;
    let payload: SetBalanceRequest = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    let service = UserService::new(state.db.clone(), state.cache.clone());
    let result = service
        .set_balance(id, payload.balance, &auth_user.role)
        .await
        .map_err(to_http)?;

    let display_balance = result.balance as f64 / BALANCE_SCALE as f64;

    Response::json(&SetBalanceResponse {
        balance: result.balance,
        display_balance,
        message: "Balance updated successfully".to_string(),
    })
}

/// Adjust balance request body (delta amount, positive = increase, negative = decrease)
#[derive(Debug, Deserialize)]
pub struct AdjustBalanceRequest {
    pub amount: i64,
}

/// Adjust balance response
#[derive(Serialize)]
pub struct AdjustBalanceResponse {
    pub balance: i64,
    pub display_balance: f64,
    pub message: String,
}

/// Adjust a user's balance — `POST /api/users/{id}/balance/adjust` (admin/system only).
pub async fn adjust_balance(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let (state, auth_user) = extract_handler_context(&req)?;
    let id: i64 = req
        .parse_param("id")
        .map_err(|_| HttpError::bad_request("Invalid or missing user ID"))?;
    let payload: AdjustBalanceRequest = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    let service = UserService::new(state.db.clone(), state.cache.clone());
    let result = service
        .adjust_balance(id, payload.amount, &auth_user.role)
        .await
        .map_err(to_http)?;

    let display_balance = result.balance as f64 / BALANCE_SCALE as f64;

    Response::json(&AdjustBalanceResponse {
        balance: result.balance,
        display_balance,
        message: "Balance adjusted successfully".to_string(),
    })
}
