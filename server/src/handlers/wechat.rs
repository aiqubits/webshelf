//! WeChat captcha-login API handlers.
//!
//! Provides the `wx_login` endpoint that verifies a captcha code obtained
//! from the WeChat Official Account and issues a JWT, as well as WeChat
//! callback handlers for server verification and message processing.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wechat_api::callback::CallbackQuery;

use crate::AppState;
use crate::handlers::auth::{expiry_cookie, token_cookie, unix_timestamp_from_now};
use crate::handlers::helpers::extract_state;
use crate::middlewares::{JWT_COOKIE, REFRESH_COOKIE};
use crate::services::wechat::WechatComponents;
use crate::utils::error::ApiError;
use webshelf_runtime::{HttpError, RequestContext, Response};

// ── wx-login endpoint ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WxLoginRequestBody {
    /// The captcha code received from the WeChat Official Account.
    pub code: String,
}

#[derive(Serialize)]
pub struct WxLoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
    pub role: String,
}

/// POST /api/public/auth/wx-login
///
/// Verify a WeChat captcha code and issue a JWT.
/// The code is obtained by sending a trigger keyword to the WeChat Official
/// Account. The openid must already be bound to a user account (via the
/// user settings page or by admin assignment).
pub async fn wx_login(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let payload: WxLoginRequestBody = req
        .parse_json_or_form()
        .await
        .map_err(HttpError::bad_request)?;

    let wechat = state
        .wechat
        .as_ref()
        .ok_or_else(|| HttpError::bad_request("WeChat login is not configured"))?;

    let result = wx_login_inner(&state, wechat, &payload).await?;
    let (resp, cookies) = result;

    let mut response = Response::json(&resp)?;
    for cookie in cookies {
        response.set_cookie(cookie);
    }
    Ok(response)
}

async fn wx_login_inner(
    state: &AppState,
    wechat: &WechatComponents,
    payload: &WxLoginRequestBody,
) -> Result<(WxLoginResponse, Vec<cookie::Cookie<'static>>), ApiError> {
    let account_id = &wechat.config.account_id;

    // 1. Look up openid from the reverse index (code → openid).
    let code_key = format!("wechat:{account_id}:code:{}", payload.code);
    let openid = wechat
        .captcha_store
        .get_opt(&code_key)
        .await?
        .ok_or_else(|| ApiError::BadRequest("Invalid or expired captcha code".to_string()))?;

    // 2. Verify the captcha via LoginService.
    let login_result = wechat
        .login_service
        .verify_and_login(account_id, &openid, &payload.code)
        .await;

    let user_id = match login_result {
        Ok(verified) => verified.user_id,
        Err(_) => {
            // Return the same generic message for all captcha-validation
            // failures (invalid code, expired code, unbound account) to
            // prevent information leakage about captcha validity.
            return Err(ApiError::BadRequest(
                "Invalid or expired captcha code".to_string(),
            ));
        }
    };

    // 3. Look up the user's role and token_version.
    let (role, token_version) = lookup_user_role_and_version(state, user_id).await?;

    // 4. Issue JWT.
    let jwt_expiry = state.config.jwt_expiry_seconds;
    let token = crate::middlewares::generate_token(
        &user_id.to_string(),
        &role,
        &state.config.jwt_secret,
        jwt_expiry,
        false,
        token_version,
    )
    .map_err(|_| ApiError::Internal("An unexpected error occurred".to_string()))?;

    let jwt_expires_at_unix = unix_timestamp_from_now(jwt_expiry)?;

    let cookies = vec![
        token_cookie(JWT_COOKIE, &token, jwt_expiry, state.config.cookie_secure),
        token_cookie(REFRESH_COOKIE, "", 0, state.config.cookie_secure),
        expiry_cookie(
            &jwt_expires_at_unix.to_string(),
            jwt_expiry,
            state.config.cookie_secure,
        ),
    ];

    tracing::debug!(user_id, "WeChat captcha login successful");

    Ok((
        WxLoginResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: jwt_expiry,
            user_id: user_id.to_string(),
            role,
        },
        cookies,
    ))
}

/// Look up a user's role and token_version by id.
async fn lookup_user_role_and_version(
    state: &AppState,
    user_id: i64,
) -> Result<(String, i32), ApiError> {
    use crate::repositories::user::Entity as UserEntity;
    use sea_orm::EntityTrait;

    let user = UserEntity::find_by_id(user_id)
        .one(state.db.write_conn())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user_id, "Database lookup failed after WeChat captcha login");
            ApiError::Internal("An unexpected error occurred".to_string())
        })?
        .ok_or_else(|| ApiError::Internal("An unexpected error occurred".to_string()))?;

    Ok((user.role, user.token_version))
}

// ── WeChat configuration status endpoint ───────────────────────────────────

#[derive(Serialize)]
pub struct WechatEnabledResponse {
    pub enabled: bool,
}

/// GET /api/public/auth/wechat-enabled
///
/// Returns whether the WeChat captcha-login feature is enabled.
/// The frontend uses this to conditionally show the captcha login tab.
pub async fn wechat_enabled(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;
    let enabled = state.wechat.is_some();
    Response::json(&WechatEnabledResponse { enabled })
}

// ── WeChat callback handlers ──────────────────────────────────────────────

/// GET /api/public/wechat/callback
///
/// WeChat server verification (echostr handshake).
/// The account_id is determined from config rather than URL path.
pub async fn wechat_callback_get(req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;

    let raw_query: HashMap<String, String> = req
        .parse_query()
        .map_err(|_| HttpError::bad_request("Missing WeChat callback query parameters"))?;
    let query = CallbackQuery::from_params(raw_query.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    let wechat = state
        .wechat
        .as_ref()
        .ok_or_else(|| HttpError::not_found("WeChat components not initialized"))?;

    // Verify signature and return echostr.
    match wechat_api::callback::handle_verification(&wechat.config, &query) {
        Ok(echostr) => {
            let mut resp = Response::new();
            resp.set_text_body(echostr);
            resp.set_content_type("text/plain");
            Ok(resp)
        }
        Err(_) => Err(HttpError::bad_request("Signature verification failed")),
    }
}

/// POST /api/public/wechat/callback
///
/// WeChat message callback — processes incoming text messages and triggers
/// captcha generation when a trigger keyword is received. The captcha code
/// is returned in the reply message so the user sees it in their WeChat chat.
///
/// IMPORTANT: This handler MUST always return 200 OK. WeChat retries messages
/// on non-200 responses, so all errors are logged and absorbed.
pub async fn wechat_callback_post(mut req: crate::ServerRequest) -> Result<Response, HttpError> {
    let state: AppState = extract_state(&req)?;

    let reply_xml = match wechat_process_callback(&state, &mut req).await {
        Ok(xml) => xml,
        Err(e) => {
            tracing::warn!("WeChat callback error (returning 200 OK to suppress retry): {e}");
            "success".to_string()
        }
    };

    let mut response = Response::new();
    response.set_text_body(&reply_xml);
    response.set_content_type("application/xml");
    Ok(response)
}

/// Inner processing for WeChat callbacks. All errors are captured as `String`
/// so the outer handler can always return 200 OK.
async fn wechat_process_callback(
    state: &AppState,
    req: &mut crate::ServerRequest,
) -> Result<String, String> {
    let raw_query: HashMap<String, String> = req
        .parse_query()
        .map_err(|e| format!("Missing/invalid WeChat callback query parameters: {e}"))?;
    let query = CallbackQuery::from_params(raw_query.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    let body_bytes = req
        .read_body_bytes()
        .await
        .map_err(|e| format!("Failed to read WeChat callback body: {e}"))?;
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    let wechat = state
        .wechat
        .as_ref()
        .ok_or_else(|| "WeChat components not initialized".to_string())?;

    // Parse callback from WeChat.
    let parsed = wechat_api::parse_callback(&wechat.config, &query, &body)
        .map_err(|e| format!("Failed to parse WeChat message: {e}"))?;

    let account_id = &wechat.config.account_id;
    let msg = &parsed.message;

    // Check if this is a text message with a trigger keyword.
    let reply_xml = if msg.msg_type == "text" {
        let content = msg.content.as_deref().unwrap_or("");
        if wechat.captcha_service.matches_trigger(content) {
            // Generate a captcha, store it, and reply with the code.
            match wechat
                .captcha_service
                .generate(account_id, &msg.from_user_name)
                .await
            {
                Ok(code) => {
                    let reply_text = format!(
                        "Your verification code: {}\nEnter this code on the login page to sign in. Valid for {} seconds.",
                        code,
                        wechat.captcha_service.captcha_ttl(),
                    );
                    wechat_api::build_text_reply(
                        &msg.from_user_name,
                        &msg.to_user_name,
                        &reply_text,
                    )
                }
                Err(wechat_api::WechatError::CooldownActive) => {
                    let reply_text = "A verification code was already sent recently. Please check your chat history or wait before requesting a new one.";
                    wechat_api::build_text_reply(&msg.from_user_name, &msg.to_user_name, reply_text)
                }
                Err(e) => {
                    tracing::error!("Failed to generate captcha: {e}");
                    let reply_text = "System busy, please try again later.";
                    wechat_api::build_text_reply(&msg.from_user_name, &msg.to_user_name, reply_text)
                }
            }
        } else {
            // No trigger keyword — send a help message.
            let reply_text = "Send \"verification code\" or \"login\" to receive a captcha code for logging in to WebShelf.";
            wechat_api::build_text_reply(&msg.from_user_name, &msg.to_user_name, reply_text)
        }
    } else {
        // Non-text messages — send a default reply.
        let reply_text = "Welcome to WebShelf! Send \"verification code\" to log in.";
        wechat_api::build_text_reply(&msg.from_user_name, &msg.to_user_name, reply_text)
    };

    Ok(reply_xml)
}
