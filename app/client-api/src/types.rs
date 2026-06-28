use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ──────────────────────────────────────────────
//  Auth types
// ──────────────────────────────────────────────

/// Login request body
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub remember: bool,
    /// WeChat captcha code (only required when wechat captcha-login is enabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captcha_code: Option<String>,
}

/// Login response
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
    /// Non-zero when the server issued a refresh token ("remember me" login).
    /// Zero / absent when the login was non-persistent.
    #[serde(default)]
    pub refresh_expires_in: Option<u64>,
}

/// Register request body
#[derive(Debug, Serialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
    /// Password confirmation (must match password).
    pub password_confirm: String,
    #[serde(default)]
    pub remember: bool,
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
//  Password reset types
// ──────────────────────────────────────────────

/// Forgot password request body
///
/// 仅含 `email` —— 服务端对未知邮箱走 dummy hash 恒定分支，
/// 永远返回 200 + 通用文案（防 enumeration），因此请求体不会泄露用户存在性。
#[derive(Debug, Serialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

/// Forgot password response
#[derive(Debug, Deserialize)]
pub struct ForgotPasswordResponse {
    pub message: String,
}

/// Reset password request body
///
/// `code` 必须是 6 位数字验证码（来自密码重置邮件），
/// `new_password` 须满足服务端复杂度校验（≥8 字符 + 复杂度）。
#[derive(Debug, Serialize)]
pub struct ResetPasswordRequest {
    pub email: String,
    pub code: String,
    pub new_password: String,
}

/// Reset password response
///
/// 与 `LoginResponse` 同形（多一个 `message`），因为服务端在事务内
/// 原子地 `token_version += 1` 后直接签发新 JWT —— 验证码校验通过
/// 即等于登录。
#[derive(Debug, Deserialize)]
pub struct ResetPasswordResponse {
    pub message: String,
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
}

/// Refresh token response
#[derive(Debug, Deserialize)]
pub struct RefreshResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
    pub refresh_expires_in: u64,
}

/// Logout response
///
/// The body is informational only — the actual session termination happens
/// via the `Set-Cookie` headers (which are invisible to JS) and the
/// server-side deletion of the refresh-token row. Clients should still
/// clear their own in-memory JWT state and the readable `webshelf_exp`
/// cookie after calling this endpoint.
#[derive(Debug, Deserialize)]
pub struct LogoutResponse {
    pub message: String,
}

// ──────────────────────────────────────────────
//  User types
// ──────────────────────────────────────────────

/// User response (mirrors server's `UserResponse`)
#[derive(Debug, Clone, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// User balance (stored as big value, 1 display unit = 10^10 stored units)
    #[serde(default)]
    pub balance: i64,
}

/// Create user request body (admin)
#[derive(Debug, Serialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
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
///
/// `new_token` 必须被调用方消费 —— 服务端在改密时原子地
/// `token_version += 1`，原 JWT 永久失效；调用方必须用 `new_token` 替换
/// 旧 token，否则下次 API 调用会 401。
#[derive(Debug, Deserialize)]
pub struct ChangePasswordResponse {
    pub message: String,
    pub new_token: String,
}

// ──────────────────────────────────────────────
//  WeChat captcha-login types
// ──────────────────────────────────────────────

/// WeChat captcha login request body
#[derive(Debug, Serialize)]
pub struct WxLoginRequest {
    pub code: String,
}

/// WeChat captcha login response
#[derive(Debug, Deserialize)]
pub struct WxLoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
}

/// WeChat enabled check response
#[derive(Debug, Deserialize)]
pub struct WechatEnabledResponse {
    pub enabled: bool,
}

// ──────────────────────────────────────────────
//  Balance types
// ──────────────────────────────────────────────

/// Set balance request body (admin/system only)
#[derive(Debug, Serialize)]
pub struct SetBalanceRequest {
    /// Balance in stored units (1 display unit = 10^10 stored units)
    pub balance: i64,
}

/// Set balance response
#[derive(Debug, Deserialize)]
pub struct SetBalanceResponse {
    pub balance: i64,
    pub display_balance: f64,
    pub message: String,
}

/// Adjust balance request body (delta amount, positive = increase, negative = decrease)
#[derive(Debug, Serialize)]
pub struct AdjustBalanceRequest {
    /// Amount in stored units (positive = increase, negative = decrease)
    pub amount: i64,
}

/// Adjust balance response
#[derive(Debug, Deserialize)]
pub struct AdjustBalanceResponse {
    pub balance: i64,
    pub display_balance: f64,
    pub message: String,
}
