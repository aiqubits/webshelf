use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ──────────────────────────────────────────────
//  Auth types
// ──────────────────────────────────────────────

/// Login request body
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
}

/// Register request body
#[derive(Debug, Serialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
}

/// Register response
#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    pub message: String,
    pub user_id: String,
    /// Whether the email is already verified.
    ///
    /// `false` means the server has issued a 6-digit verification code and
    /// the user must complete email verification before being able to log in.
    /// `true` means the server has either auto-verified the user (no SMTP
    /// configured, or send failed) or the user was already verified.
    ///
    /// `#[serde(default)]` preserves compatibility with older server responses
    /// or fixtures that do not include the field — it will deserialize as
    /// `false`, which is the safer default (forces the frontend to show the
    /// verification UI rather than silently auto-login).
    #[serde(default)]
    pub email_verified: bool,
}

/// Verify email request body
#[derive(Debug, Serialize)]
pub struct VerifyEmailRequest {
    pub email: String,
    pub code: String,
}

/// Verify email response
#[derive(Debug, Deserialize)]
pub struct VerifyEmailResponse {
    pub message: String,
}

/// Resend verification code request body
#[derive(Debug, Serialize)]
pub struct ResendCodeRequest {
    pub email: String,
}

/// Resend verification code response
#[derive(Debug, Deserialize)]
pub struct ResendCodeResponse {
    pub message: String,
}

// ──────────────────────────────────────────────
//  User types
// ──────────────────────────────────────────────

/// User response (mirrors server's `UserResponse`)
#[derive(Debug, Clone, Deserialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create user request body (admin)
#[derive(Debug, Serialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    pub name: String,
}

/// Update user request body (admin — all fields optional)
#[derive(Debug, Serialize)]
pub struct UpdateUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Paginated list of users
#[derive(Debug, Deserialize)]
pub struct PaginatedUsersResponse {
    pub items: Vec<UserResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

// ──────────────────────────────────────────────
//  Health types
// ──────────────────────────────────────────────

/// Health check response
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Generic delete response
#[derive(Debug, Deserialize)]
pub struct DeleteResponse {
    pub message: String,
}

// ──────────────────────────────────────────────
//  Self-service types
// ──────────────────────────────────────────────

/// Change password request body
#[derive(Debug, Serialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Change password response
#[derive(Debug, Deserialize)]
pub struct ChangePasswordResponse {
    pub message: String,
}
