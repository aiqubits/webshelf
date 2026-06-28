use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::AppState;
use crate::handlers::helpers::extract_state;
use crate::middlewares::{EXPIRY_COOKIE, JWT_COOKIE, REFRESH_COOKIE};
use crate::repositories::user::CreateUserInput;
use crate::services::auth::{AuthService, LoginRequest, LoginResponse};
use crate::services::password_reset::{PasswordResetError, PasswordResetService};
use crate::services::user::UserService;
use crate::services::verification::{VerificationError, VerificationService};
use crate::utils::error::ApiError;
use crate::utils::validator::check_password_strength;
use sha2::Digest;
use webshelf_runtime::{HttpError, RequestContext, Response};
use wechat_api::error::WechatError;

/// Build an httpOnly token cookie.
pub(crate) fn token_cookie(
    name: &str,
    value: &str,
    max_age_secs: u64,
    secure: bool,
) -> cookie::Cookie<'static> {
    let mut c = cookie::Cookie::new(name.to_owned(), value.to_owned());
    c.set_path("/");
    c.set_max_age(cookie::time::Duration::seconds(max_age_secs as i64));
    c.set_http_only(true);
    c.set_same_site(cookie::SameSite::Strict);
    if secure {
        c.set_secure(true);
    }
    c
}

/// Build a readable expiry cookie.
pub(crate) fn expiry_cookie(
    value: &str,
    max_age_secs: u64,
    secure: bool,
) -> cookie::Cookie<'static> {
    let mut c = cookie::Cookie::new(EXPIRY_COOKIE.to_owned(), value.to_owned());
    c.set_path("/");
    c.set_max_age(cookie::time::Duration::seconds(max_age_secs as i64));
    c.set_same_site(cookie::SameSite::Strict);
    if secure {
        c.set_secure(true);
    }
    c
}

/// Clear all auth cookies by setting them with Max-Age=0.
pub(crate) fn clear_auth_cookies(secure: bool) -> Vec<cookie::Cookie<'static>> {
    let clear = |name: &str, http_only: bool| {
        let mut c = cookie::Cookie::new(name.to_owned(), String::new());
        c.set_path("/");
        c.set_max_age(cookie::time::Duration::seconds(0));
        if http_only {
            c.set_http_only(true);
        }
        c.set_same_site(cookie::SameSite::Strict);
        if secure {
            c.set_secure(true);
        }
        c
    };
    vec![
        clear(JWT_COOKIE, true),
        clear(REFRESH_COOKIE, true),
        clear(EXPIRY_COOKIE, false),
    ]
}

/// Login request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 1, message = "password is required"))]
    password: String,

    #[serde(default)]
    remember: bool,

    /// WeChat captcha code — required when wechat captcha-login is enabled.
    #[serde(default)]
    captcha_code: Option<String>,
}

/// Login endpoint
pub async fn login(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let payload: LoginRequestBody = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    let result = login_inner(&state, &payload).await?;
    let (login_resp, cookies) = result;

    let mut response = Response::json(&login_resp)?;
    for cookie in cookies {
        response.set_cookie(cookie);
    }
    Ok(response)
}

async fn login_inner(
    state: &AppState,
    payload: &LoginRequestBody,
) -> Result<(LoginResponse, Vec<cookie::Cookie<'static>>), ApiError> {
    payload.validate()?;

    // ── WeChat captcha verification (must precede password login) ──────
    // Verify the captcha FIRST so that a failed captcha does not leave
    // behind a stored refresh token from the password login step (the
    // login service persists the refresh token in a transaction).
    // On success the captcha is consumed (one-shot); if the subsequent
    // password login fails, the user simply requests a new captcha from
    // the WeChat Official Account — a cheaper cost than an orphaned
    // refresh token row.
    //
    // When the WeChat captcha-login feature is enabled, the email+password
    // login also requires a valid captcha code obtained from the WeChat
    // Official Account.  The captcha-bound user must match the authenticated
    // user — this prevents a captcha obtained by one user from being used to
    // log in as another user.
    let captcha_user_id = if let Some(wechat) = state.wechat.as_ref() {
        let captcha_code = payload.captcha_code.as_deref().unwrap_or("");
        if captcha_code.is_empty() {
            return Err(ApiError::BadRequest(
                "Invalid or expired captcha code".to_string(),
            ));
        }

        let account_id = &wechat.config.account_id;

        // 1. Look up openid from reverse index (code → openid).
        let code_key = format!("wechat:{account_id}:code:{}", captcha_code);
        let openid = wechat
            .captcha_store
            .get_opt(&code_key)
            .await?
            .ok_or_else(|| ApiError::BadRequest("Invalid or expired captcha code".to_string()))?;

        // 2. Verify captcha via LoginService (consumes it on success).
        let verified = wechat
            .login_service
            .verify_and_login(account_id, &openid, captcha_code)
            .await
            .map_err(|e| {
                // Log internal/unexpected errors at error level; expected
                // captcha failures at warn level — all return the same
                // generic message to prevent information leakage.
                match &e {
                    WechatError::Internal(_)
                    | WechatError::Store(_)
                    | WechatError::ApiRequest(_)
                    | WechatError::ApiBusiness { .. } => {
                        tracing::error!(error = %e, "WeChat verify_and_login failed");
                    }
                    _ => {
                        tracing::warn!(error = %e, "WeChat captcha verification failed");
                    }
                }
                ApiError::BadRequest("Invalid or expired captcha code".to_string())
            })?;

        Some(verified.user_id)
    } else {
        None
    };

    // ── Password login ───────────────────────────────────────────────
    let service = AuthService::new(
        state.db.clone(),
        state.config.jwt_secret.clone(),
        state.config.jwt_expiry_seconds,
        state.config.jwt_remember_expiry_seconds,
        state.config.refresh_token_expiry_seconds,
    );

    let result = service
        .login(LoginRequest {
            email: payload.email.to_lowercase(),
            password: payload.password.clone(),
            remember: payload.remember,
        })
        .await?;

    // ── Post-login captcha-bound user check ──────────────────────────
    if let Some(captcha_user_id) = captcha_user_id {
        if captcha_user_id.to_string() != result.user_id {
            return Err(ApiError::BadRequest(
                "Invalid or expired captcha code".to_string(),
            ));
        }

        tracing::debug!(
            user_id = %result.user_id,
            "WeChat captcha verified for email+password login"
        );
    }

    let jwt_max_age = result.expires_in;
    let jwt_expires_at_unix = unix_timestamp_from_now(jwt_max_age)?;

    // Refresh cookie is only meaningful for "remember me" sessions. For
    // non-remembered logins the service returns an empty refresh_token —
    // emit a Max-Age=0 cookie so any stale refresh cookie from a previous
    // session is purged by the browser.
    let refresh_cookie = if result.refresh_token.is_empty() {
        token_cookie(REFRESH_COOKIE, "", 0, state.config.cookie_secure)
    } else {
        token_cookie(
            REFRESH_COOKIE,
            &result.refresh_token,
            result.refresh_expires_in,
            state.config.cookie_secure,
        )
    };

    let cookies = vec![
        token_cookie(
            JWT_COOKIE,
            &result.token,
            jwt_max_age,
            state.config.cookie_secure,
        ),
        refresh_cookie,
        expiry_cookie(
            &jwt_expires_at_unix.to_string(),
            result.refresh_expires_in.max(jwt_max_age),
            state.config.cookie_secure,
        ),
    ];

    Ok((result, cookies))
}

/// Compute the Unix timestamp `seconds_from_now` seconds in the future.
/// Used to write the JWT's absolute expiry into the readable `webshelf_exp`
/// cookie, so the frontend can compare it against `Date.now()` / 1000
/// without having to re-decode the JWT.
pub(crate) fn unix_timestamp_from_now(seconds_from_now: u64) -> Result<i64, ApiError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;
    let ts = now.as_secs() as i64 + seconds_from_now as i64;
    Ok(ts)
}

/// Register request with validation
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequestBody {
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

    /// 二次密码确认，后端校验是否与 password 一致
    #[serde(default)]
    password_confirm: String,

    /// Ignored by the register endpoint (registration never issues tokens
    /// directly); passed through for client contract consistency so that
    /// serialization frameworks with deny_unknown_fields do not break.
    #[serde(default)]
    #[allow(dead_code)]
    remember: bool,
}

/// Register response
#[derive(Serialize)]
pub struct RegisterResponse {
    message: String,
    user_id: String,
    /// Whether the email is already verified.
    /// When the email service is not configured, registration auto-verifies.
    email_verified: bool,
}

/// Register endpoint
pub async fn register(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let payload: RegisterRequestBody = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    // Manually convert ValidationErrors to HttpError (orphan rule prevents direct From impl)
    payload
        .validate()
        .inspect_err(|e| {
            tracing::warn!("Registration validation failed: {:?}", e);
        })
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    // 二次密码校验：确认前端传入的 password_confirm 与 password 一致
    if payload.password != payload.password_confirm {
        return Err(HttpError::bad_request("passwords do not match"));
    }

    // check_password_strength returns ApiError; ? converts to HttpError via From<ApiError> for HttpError
    check_password_strength(&payload.password)?;

    // Normalize email to lowercase once at the entry point to avoid
    // redundant normalization in multiple downstream call sites.
    let email = payload.email.to_lowercase();

    let service = UserService::new(state.db.clone(), state.cache.clone());
    // UserError -> HttpError via ApiError
    let user = service
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: payload.password,
                name: payload.name,
                role: None,
            },
            "user",
        )
        .await
        .map_err(|e| HttpError::from(ApiError::from(e)))?;

    let verification = VerificationService::new(state.db.clone(), state.email.clone());
    let (message, email_verified) = match verification.send_verification_email(&email).await {
        Ok(()) => ("Verification code sent to your email".to_string(), false),
        Err(VerificationError::EmailNotConfigured) => {
            tracing::warn!("Email service not configured — auto-verifying user");
            if let Err(e) = verification.auto_verify(&email).await {
                tracing::error!("Failed to auto-verify user: {:?}", e);
                return Err(HttpError::internal(
                    "Registration failed due to an internal error. Please try again later.",
                ));
            }
            ("User registered successfully".to_string(), true)
        }
        Err(e) => {
            // When SMTP is configured but the send fails (transient network error,
            // etc.), auto-verify the user as a fallback instead of deleting the
            // account.  This avoids orphan accounts that can neither log in
            // (email_verified=false) nor re-register (email already taken), and
            // eliminates the crash window between insert and manual DELETE.
            tracing::error!("Failed to send verification email: {:?}", e);
            if let Err(verify_err) = verification.auto_verify(&email).await {
                tracing::error!(
                    "Failed to auto-verify after email failure: {:?}",
                    verify_err
                );
                return Err(HttpError::internal(
                    "Registration failed due to an internal error. Please try again later.",
                ));
            }
            ("User registered successfully. Note: verification email could not be sent, but your account is active.".to_string(), true)
        }
    };

    tracing::trace!("User registered successfully with id: {}", user.id);
    Response::json(&RegisterResponse {
        message,
        user_id: user.id.to_string(),
        email_verified,
    })
}

/// Verify email request
#[derive(Debug, Deserialize, Validate)]
pub struct VerifyEmailRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 6, max = 6, message = "code must be 6 characters"))]
    code: String,
}

/// Verify email response
#[derive(Serialize)]
pub struct VerifyEmailResponse {
    message: String,
}

/// Verify email endpoint
pub async fn verify_email(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let payload: VerifyEmailRequestBody = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    // Manually convert ValidationErrors to HttpError
    payload
        .validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    // Reject non-numeric codes early to avoid wasting Argon2 CPU
    // on obviously invalid inputs.
    if !payload.code.chars().all(|c| c.is_ascii_digit()) {
        return Err(HttpError::bad_request("code must be 6 digits"));
    }

    // Normalize email to lowercase at the entry point.
    let email = payload.email.to_lowercase();

    let service = VerificationService::new(state.db.clone(), state.email.clone());
    // VerificationError -> HttpError via ApiError
    service
        .verify_email(&email, &payload.code)
        .await
        .map_err(|e| HttpError::from(ApiError::from(e)))?;

    Response::json(&VerifyEmailResponse {
        message: "Email verified successfully".to_string(),
    })
}

/// Resend verification code request
#[derive(Debug, Deserialize, Validate)]
pub struct ResendCodeRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,
}

/// Resend code response
#[derive(Serialize)]
pub struct ResendCodeResponse {
    message: String,
}

/// Resend verification code endpoint
pub async fn resend_code(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let payload: ResendCodeRequestBody = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    // Manually convert ValidationErrors to HttpError
    payload
        .validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    // Normalize email to lowercase at the entry point.
    let email = payload.email.to_lowercase();

    let service = VerificationService::new(state.db.clone(), state.email.clone());

    // Anti-enumeration: swallow `TooSoon` to prevent leaking whether an
    // email is registered via the cooldown window.  Non-existent users
    // always get 200; existent users within cooldown must also get 200
    // (the same generic message) so that an attacker cannot distinguish
    // the two cases by measuring response differences between first and
    // second request.
    if let Err(e) = service.resend_code(&email).await {
        if matches!(e, VerificationError::TooSoon) {
            tracing::info!(
                "Resend-code within cooldown for {} (TooSoon swallowed)",
                email
            );
        } else {
            return Err(HttpError::from(ApiError::from(e)));
        }
    }

    Response::json(&ResendCodeResponse {
        message: "If that email is registered, a new verification code has been sent".to_string(),
    })
}

/// Forgot-password request — initiate a password-reset email.
///
/// Always returns 200 OK on a syntactically valid email to prevent
/// user enumeration; the response body is identical regardless of whether
/// the email is registered. Cooldown errors are swallowed (still 200) so
/// that attackers cannot distinguish registered from unregistered emails
/// by sending a second request within the cooldown window. SMTP
/// configuration failures surface as 503 only for registered emails.
#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,
}

#[derive(Serialize)]
pub struct ForgotPasswordResponse {
    message: String,
}

pub async fn forgot_password(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let payload: ForgotPasswordRequestBody = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    // Manually convert ValidationErrors to HttpError
    payload
        .validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    let email = payload.email.to_lowercase();

    let service = PasswordResetService::new(state.db.clone(), state.email.clone());

    // Anti-enumeration: swallow `TooSoon` to prevent leaking whether an
    // email is registered via the cooldown window. Non-existent users
    // always get 200; existent users within cooldown must also get 200
    // (the same generic message) so that an attacker cannot distinguish
    // the two cases by measuring response differences between first and
    // second request.
    if let Err(e) = service.request_reset(&email).await {
        if matches!(e, PasswordResetError::TooSoon) {
            tracing::info!(
                "Forgot-password within cooldown for {} (TooSoon swallowed)",
                email
            );
        } else {
            return Err(HttpError::from(ApiError::from(e)));
        }
    }

    // The message intentionally does not reveal whether the email is registered.
    Response::json(&ForgotPasswordResponse {
        message: "If that email is registered, a reset code has been sent".to_string(),
    })
}

/// Reset-password request — consume the verification code sent in the
/// reset email and replace the user's password.
///
/// On success, returns a fresh JWT so the user is auto-logged-in.
#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordRequestBody {
    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 6, max = 6, message = "code must be 6 digits"))]
    code: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    new_password: String,
}

#[derive(Serialize)]
pub struct ResetPasswordResponse {
    message: String,
    /// Fresh JWT issued after the password is replaced.
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
}

pub async fn reset_password(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let payload: ResetPasswordRequestBody = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    let result = reset_password_inner(&state, &payload).await?;
    let (resp, cookies) = result;

    let mut response = Response::json(&resp)?;
    for cookie in cookies {
        response.set_cookie(cookie);
    }
    Ok(response)
}

async fn reset_password_inner(
    state: &AppState,
    payload: &ResetPasswordRequestBody,
) -> Result<(ResetPasswordResponse, Vec<cookie::Cookie<'static>>), ApiError> {
    payload.validate()?;
    check_password_strength(&payload.new_password)?;

    // Reject non-numeric codes early to avoid wasting Argon2 CPU
    // on obviously invalid inputs.
    if !payload.code.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::BadRequest("code must be 6 digits".to_string()));
    }

    let email = payload.email.to_lowercase();

    let service = PasswordResetService::new(state.db.clone(), state.email.clone());
    let outcome = service
        .reset_password(&email, &payload.code, &payload.new_password)
        .await?;

    let new_token = crate::middlewares::generate_token(
        &outcome.user_id.to_string(),
        &outcome.role,
        &state.config.jwt_secret,
        state.config.jwt_expiry_seconds,
        false,
        outcome.token_version,
    )
    .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;

    let jwt_max_age = state.config.jwt_expiry_seconds;
    let jwt_expires_at_unix = unix_timestamp_from_now(jwt_max_age)?;

    // Refresh tokens have been revoked during password reset. Clear any
    // stale refresh cookie the browser may still hold.
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

    tracing::info!("Password reset completed for user {}", outcome.user_id);
    Ok((
        ResetPasswordResponse {
            message: "Password reset successfully".to_string(),
            token: new_token,
            token_type: "Bearer".to_string(),
            expires_in: state.config.jwt_expiry_seconds,
            user_id: outcome.user_id.to_string(),
            role: outcome.role,
        },
        cookies,
    ))
}

/// Refresh-token request — exchange a valid refresh token cookie for a new JWT.
///
/// The refresh token is read from the `webshelf_refresh` httpOnly cookie.
/// On success, the old refresh token is deleted and a new one is issued
/// (rotation), and new cookies are set.
#[derive(Serialize)]
pub struct RefreshResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
    #[serde(skip_serializing)]
    pub refresh_token: String,
    pub refresh_expires_in: u64,
}

pub async fn refresh(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;

    let result = refresh_inner(&state, &req).await?;
    let (refresh_resp, cookies) = result;

    let mut response = Response::json(&refresh_resp)?;
    for cookie in cookies {
        response.set_cookie(cookie);
    }
    Ok(response)
}

async fn refresh_inner(
    state: &AppState,
    req: &crate::ServerRequest,
) -> Result<(RefreshResponse, Vec<cookie::Cookie<'static>>), ApiError> {
    let refresh_token = req
        .cookie(REFRESH_COOKIE)
        .ok_or_else(|| ApiError::Unauthorized("Missing refresh token cookie".to_string()))?;

    let token_hash = hex::encode(sha2::Sha256::digest(refresh_token.as_bytes()));

    let service = AuthService::new(
        state.db.clone(),
        state.config.jwt_secret.clone(),
        state.config.jwt_expiry_seconds,
        state.config.jwt_remember_expiry_seconds,
        state.config.refresh_token_expiry_seconds,
    );

    // Generate new refresh token before the atomic rotation
    let (raw_refresh, new_hash) = AuthService::generate_refresh_token();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;
    let refresh_expires_at = chrono::DateTime::from_timestamp(
        (now.as_secs() + state.config.refresh_token_expiry_seconds) as i64,
        0,
    )
    .ok_or_else(|| ApiError::Internal("An unexpected error occurred".to_string()))?;

    // Atomic rotation: validate old + delete old + insert new in one transaction.
    // Returns None when the token is not found, already expired, or already
    // consumed — all three cases are treated as unauthorised. The refresh token
    // is the sole authority for granting a new JWT; we do NOT fall back to the
    // JWT cookie, because doing so would bypass expiry/revocation enforcement.
    let (user_id, role, token_version) = service
        .rotate_refresh_token(&token_hash, &new_hash, refresh_expires_at)
        .await
        .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?
        .ok_or_else(|| ApiError::Unauthorized("Invalid or expired refresh token".to_string()))?;

    // Issue new JWT — use remember expiry since the refresh token's existence
    // implies the user originally opted into a persistent session.
    let new_token = crate::middlewares::generate_token(
        &user_id.to_string(),
        &role,
        &state.config.jwt_secret,
        state.config.jwt_remember_expiry_seconds,
        true,
        token_version,
    )
    .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;

    let jwt_max_age = state.config.jwt_remember_expiry_seconds;
    let refresh_max_age = state.config.refresh_token_expiry_seconds;
    let jwt_expires_at_unix = unix_timestamp_from_now(jwt_max_age)?;

    let cookies = vec![
        token_cookie(
            JWT_COOKIE,
            &new_token,
            jwt_max_age,
            state.config.cookie_secure,
        ),
        token_cookie(
            REFRESH_COOKIE,
            &raw_refresh,
            refresh_max_age,
            state.config.cookie_secure,
        ),
        expiry_cookie(
            &jwt_expires_at_unix.to_string(),
            refresh_max_age.max(jwt_max_age),
            state.config.cookie_secure,
        ),
    ];

    tracing::info!("Token refreshed for user {}", user_id);

    Ok((
        RefreshResponse {
            token: new_token,
            token_type: "Bearer".to_string(),
            expires_in: jwt_max_age,
            user_id: user_id.to_string(),
            role,
            refresh_token: raw_refresh,
            refresh_expires_in: refresh_max_age,
        },
        cookies,
    ))
}

/// Single-session logout — `POST /api/public/auth/logout`.
///
/// Revokes the current session's refresh token (if a `webshelf_refresh`
/// cookie is present) and clears all three auth cookies. The endpoint is
/// public — the refresh cookie alone is sufficient to identify the DB row
/// to delete, so a frontend can always log itself out even after its
/// in-memory JWT has expired. Idempotent: missing cookie or already-revoked
/// row both still return 200 and clear cookies.
#[derive(Serialize)]
pub struct LogoutResponse {
    pub message: String,
}

pub async fn logout(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;

    // Pull the refresh cookie (if any). httpOnly, so we read it from the
    // request headers here instead of trying to round-trip it through the
    // client. The JWT cookie is intentionally NOT consulted — by the time
    // the frontend calls logout, the JWT may already be expired.
    let refresh_token = req.cookie(REFRESH_COOKIE);

    if let Some(raw_refresh) = refresh_token
        && !raw_refresh.is_empty()
    {
        let token_hash = hex::encode(sha2::Sha256::digest(raw_refresh.as_bytes()));

        let service = AuthService::new(
            state.db.clone(),
            state.config.jwt_secret.clone(),
            state.config.jwt_expiry_seconds,
            state.config.jwt_remember_expiry_seconds,
            state.config.refresh_token_expiry_seconds,
        );
        // Best-effort: a failure to delete the row is not fatal — the
        // cookie clear still ends the browser-side session, and a
        // missing/expired row simply means rows_affected = 0.
        if let Err(e) = service.delete_refresh_token(&*state.db, &token_hash).await {
            tracing::warn!(
                "Failed to delete refresh token during logout (continuing): {:?}",
                e
            );
        } else {
            tracing::info!("Refresh token revoked via logout endpoint");
        }
    }

    let cookies = clear_auth_cookies(state.config.cookie_secure);

    let mut response = Response::json(&LogoutResponse {
        message: "Logged out successfully".to_string(),
    })?;

    for cookie in cookies {
        response.set_cookie(cookie);
    }

    Ok(response)
}
