//! 客户端 API 工厂与 401 拦截辅助。
//!
//! - `make_client()` 根据编译目标返回合适的 `client_api::Client`。
//! - `is_unauth(err)` 判定一个 `ClientError` 是否表示 token 失效。
//! - `handle_unauth(err, auth, nav, log_bus)` 检测到 401 时执行 logout + 跳转 `/auth`
//!   并写入 `LogKind::Important` 日志，与 `TokenExpiryGuard::fire_expiry` 行为对齐。
//! - `humanize_error(err, ctx, lang)` 将 API 错误翻译为当前语言提示。

mod client;

pub use client::make_client;

use client_api::ClientError;
use dioxus::prelude::dioxus_router::Navigator;
use i18n::Language;

use crate::Route;
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, LogKind};

/// 错误翻译上下文 —— 不同视图对同一 HTTP 状态码可能有不同文案。
pub enum ErrorContext {
    /// 登录 / 注册页面：401 → "邮箱或密码错误"
    Auth,
    /// 用户管理页面：401 → "未登录或会话已过期"，额外支持 403 / 404
    UserManagement,
    /// 邮件验证页面：统一 400 文案（与服务端 anti-enumeration 对齐——
    /// 不区分"用户不存在"、"码错误"、"码过期"、"超过尝试上限"）
    EmailVerification,
    /// 密码重置页面：与 verify-email 类似，服务端对所有失败分支统一
    /// 400 + 通用文案以防 enumeration / 凭证探测。
    PasswordReset,
}

/// 将 `ClientError` 翻译为当前语言提示，根据 `ctx` 差异化状态码文案。
pub fn humanize_error(err: &ClientError, ctx: ErrorContext, lang: Language) -> String {
    match err {
        ClientError::Network(msg) => match lang {
            Language::En => format!("Network error: {msg}"),
            Language::Zh => format!("网络异常: {msg}"),
        },
        ClientError::ServerError(status, body) => match lang {
            Language::En => format!("Server error (HTTP {status}): {body}"),
            Language::Zh => format!("服务器错误 (HTTP {status}): {body}"),
        },
        ClientError::Other(status, body) => {
            let json = serde_json::from_str::<serde_json::Value>(body).ok();
            let code = json
                .as_ref()
                .and_then(|v| v.get("error").and_then(|c| c.as_str().map(String::from)))
                .unwrap_or_default();
            let msg = json
                .as_ref()
                .and_then(|v| v.get("message").and_then(|m| m.as_str().map(String::from)))
                .unwrap_or_else(|| body.clone());
            match lang {
                Language::En => match ctx {
                    ErrorContext::Auth => match (status, code.as_str()) {
                        (401, _) => "Invalid email or password".to_string(),
                        (_, "validation_error") => format!("Validation error: {msg}"),
                        (_, "conflict") => "Email already registered".to_string(),
                        _ => format!("Request failed (HTTP {status}): {msg}"),
                    },
                    ErrorContext::UserManagement => match (status, code.as_str()) {
                        (401, _) => "Not logged in or session expired".to_string(),
                        (403, _) => "Insufficient permissions (admin required)".to_string(),
                        (404, _) => "User not found".to_string(),
                        (_, "validation_error") => format!("Validation error: {msg}"),
                        (_, "conflict") => {
                            "Operation conflict (email already exists or constraint violation)"
                                .to_string()
                        }
                        _ => format!("Request failed (HTTP {status}): {msg}"),
                    },
                    ErrorContext::EmailVerification => match (status, code.as_str()) {
                        (400, _) => "Invalid or expired verification code".to_string(),
                        (503, _) => "Email service not configured".to_string(),
                        _ => format!("Request failed (HTTP {status}): {msg}"),
                    },
                    ErrorContext::PasswordReset => match (status, code.as_str()) {
                        (400, _) => "Invalid or expired reset code".to_string(),
                        (503, _) => "Password reset is currently unavailable".to_string(),
                        _ => format!("Request failed (HTTP {status}): {msg}"),
                    },
                },
                Language::Zh => match ctx {
                    ErrorContext::Auth => match (status, code.as_str()) {
                        (401, _) => "邮箱或密码错误".to_string(),
                        (_, "validation_error") => format!("参数错误: {msg}"),
                        (_, "conflict") => "该邮箱已注册".to_string(),
                        _ => format!("请求失败 (HTTP {status}): {msg}"),
                    },
                    ErrorContext::UserManagement => match (status, code.as_str()) {
                        (401, _) => "未登录或会话已过期".to_string(),
                        (403, _) => "权限不足 (需 admin)".to_string(),
                        (404, _) => "用户不存在".to_string(),
                        (_, "validation_error") => format!("参数错误: {msg}"),
                        (_, "conflict") => "操作冲突（邮箱已存在或违反约束）".to_string(),
                        _ => format!("请求失败 (HTTP {status}): {msg}"),
                    },
                    ErrorContext::EmailVerification => match (status, code.as_str()) {
                        (400, _) => "验证码错误或已过期".to_string(),
                        (503, _) => "邮件服务未配置".to_string(),
                        _ => format!("请求失败 (HTTP {status}): {msg}"),
                    },
                    ErrorContext::PasswordReset => match (status, code.as_str()) {
                        (400, _) => "重置验证码无效或已过期".to_string(),
                        (503, _) => "密码重置功能暂不可用".to_string(),
                        _ => format!("请求失败 (HTTP {status}): {msg}"),
                    },
                },
            }
        }
        ClientError::RateLimited(_) => match lang {
            Language::En => "Too many requests, please try again later".to_string(),
            Language::Zh => "请求过于频繁，请稍后再试".to_string(),
        },
        ClientError::Deserialization(msg) => match lang {
            Language::En => format!("Response parse failed: {msg}"),
            Language::Zh => format!("响应解析失败: {msg}"),
        },
        ClientError::Config(msg) => match lang {
            Language::En => format!("Client configuration error: {msg}"),
            Language::Zh => format!("客户端配置错误: {msg}"),
        },
        _ => match lang {
            Language::En => format!("Unknown error: {err}"),
            Language::Zh => format!("未知错误: {err}"),
        },
    }
}

/// 判定一个 `ClientError` 是否代表 token 失效（HTTP 401）。
pub fn is_unauth(err: &ClientError) -> bool {
    matches!(err, ClientError::Other(401, _))
}

/// 若 `err` 为 401 或 403，执行相应导航并返回 `true`：
/// - 401：调用 `auth.logout_async()` 撤销后端 refresh token，再跳 `/auth`；
/// - 403：仅跳 `/`（已认证但权限不足，如 JWT role 被篡改）。
///
/// 否则返回 `false`，让调用方继续处理业务错误。
///
/// 视图层模式：
/// ```ignore
/// if let Err(e) = client.list_users(1, 20).await {
///     if handle_unauth(&e, auth, nav, log_bus).await { return; }
///     // 处理业务错误...
/// }
/// ```
pub async fn handle_unauth(
    err: &ClientError,
    mut auth: AuthState,
    nav: Navigator,
    mut log_bus: LogBus,
) -> bool {
    if is_unauth(err) {
        log_bus.push(
            HttpMethod::Post,
            "/auth/logout (session expired via 401)".to_string(),
            "401".to_string(),
            LogKind::Important,
        );
        // 401 意味着 JWT 已被服务端拒绝 —— 通过 logout 端点同步撤销
        // refresh token，避免 refresh cookie 仍可用来换发新 JWT 的悬空会话。
        auth.logout_async().await;
        nav.replace(Route::LoginLanding {});
        true
    } else if matches!(err, ClientError::Other(403, _)) {
        log_bus.push(
            HttpMethod::Post,
            "/auth/forbidden (JWT admin scope mismatch)".to_string(),
            "403".to_string(),
            LogKind::Important,
        );
        nav.replace(Route::Dashboard {});
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_unauth ────────────────────────────────────────

    #[test]
    fn is_unauth_401_returns_true() {
        let err = ClientError::Other(401, "unauthorized".into());
        assert!(is_unauth(&err));
    }

    #[test]
    fn is_unauth_403_returns_false() {
        let err = ClientError::Other(403, "forbidden".into());
        assert!(!is_unauth(&err));
    }

    #[test]
    fn is_unauth_400_returns_false() {
        let err = ClientError::Other(400, "bad request".into());
        assert!(!is_unauth(&err));
    }

    #[test]
    fn is_unauth_network_returns_false() {
        let err = ClientError::Network("timeout".into());
        assert!(!is_unauth(&err));
    }

    #[test]
    fn is_unauth_server_error_401_returns_false() {
        let err = ClientError::ServerError(401, "Unauthorized".into());
        assert!(!is_unauth(&err));
    }

    // ── humanize_error: Auth context ─────────────────────

    #[test]
    fn humanize_auth_401_zh() {
        let err = ClientError::Other(401, r#"{"error":"unauthorized"}"#.into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::Zh);
        assert_eq!(msg, "邮箱或密码错误");
    }

    #[test]
    fn humanize_auth_401_en() {
        let err = ClientError::Other(401, r#"{"error":"unauthorized"}"#.into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::En);
        assert_eq!(msg, "Invalid email or password");
    }

    #[test]
    fn humanize_auth_validation_error_zh() {
        let err = ClientError::Other(
            400,
            r#"{"error":"validation_error","message":"email is invalid"}"#.into(),
        );
        let msg = humanize_error(&err, ErrorContext::Auth, Language::Zh);
        assert!(msg.contains("参数错误"));
    }

    #[test]
    fn humanize_auth_validation_error_en() {
        let err = ClientError::Other(
            400,
            r#"{"error":"validation_error","message":"email is invalid"}"#.into(),
        );
        let msg = humanize_error(&err, ErrorContext::Auth, Language::En);
        assert!(msg.contains("Validation error"));
    }

    #[test]
    fn humanize_auth_conflict_zh() {
        let err = ClientError::Other(
            409,
            r#"{"error":"conflict","message":"email already registered"}"#.into(),
        );
        let msg = humanize_error(&err, ErrorContext::Auth, Language::Zh);
        assert_eq!(msg, "该邮箱已注册");
    }

    #[test]
    fn humanize_auth_conflict_en() {
        let err = ClientError::Other(
            409,
            r#"{"error":"conflict","message":"email already registered"}"#.into(),
        );
        let msg = humanize_error(&err, ErrorContext::Auth, Language::En);
        assert_eq!(msg, "Email already registered");
    }

    #[test]
    fn humanize_auth_network_zh() {
        let err = ClientError::Network("connection refused".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::Zh);
        assert!(msg.contains("网络异常"));
    }

    #[test]
    fn humanize_auth_network_en() {
        let err = ClientError::Network("connection refused".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::En);
        assert!(msg.contains("Network error"));
    }

    // ── humanize_error: UserManagement context ───────────

    #[test]
    fn humanize_usermgmt_401_zh() {
        let err = ClientError::Other(401, r#"{"error":"unauthorized"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::Zh);
        assert_eq!(msg, "未登录或会话已过期");
    }

    #[test]
    fn humanize_usermgmt_401_en() {
        let err = ClientError::Other(401, r#"{"error":"unauthorized"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::En);
        assert_eq!(msg, "Not logged in or session expired");
    }

    #[test]
    fn humanize_usermgmt_403_zh() {
        let err = ClientError::Other(403, r#"{"error":"forbidden"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::Zh);
        assert_eq!(msg, "权限不足 (需 admin)");
    }

    #[test]
    fn humanize_usermgmt_403_en() {
        let err = ClientError::Other(403, r#"{"error":"forbidden"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::En);
        assert_eq!(msg, "Insufficient permissions (admin required)");
    }

    #[test]
    fn humanize_usermgmt_404_zh() {
        let err = ClientError::Other(404, r#"{"error":"not_found"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::Zh);
        assert_eq!(msg, "用户不存在");
    }

    #[test]
    fn humanize_usermgmt_404_en() {
        let err = ClientError::Other(404, r#"{"error":"not_found"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::En);
        assert_eq!(msg, "User not found");
    }

    #[test]
    fn humanize_usermgmt_server_error_zh() {
        let err = ClientError::ServerError(500, "Internal Server Error".into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::Zh);
        assert!(msg.contains("服务器错误"));
    }

    #[test]
    fn humanize_usermgmt_server_error_en() {
        let err = ClientError::ServerError(500, "Internal Server Error".into());
        let msg = humanize_error(&err, ErrorContext::UserManagement, Language::En);
        assert!(msg.contains("Server error"));
    }

    #[test]
    fn humanize_rate_limited_zh() {
        let err = ClientError::RateLimited("Too many requests".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::Zh);
        assert_eq!(msg, "请求过于频繁，请稍后再试");
    }

    #[test]
    fn humanize_rate_limited_en() {
        let err = ClientError::RateLimited("Too many requests".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::En);
        assert_eq!(msg, "Too many requests, please try again later");
    }

    #[test]
    fn humanize_deserialization_zh() {
        let err = ClientError::Deserialization("expected `{`".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::Zh);
        assert!(msg.contains("响应解析失败"));
    }

    #[test]
    fn humanize_deserialization_en() {
        let err = ClientError::Deserialization("expected `{`".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::En);
        assert!(msg.contains("Response parse failed"));
    }

    #[test]
    fn humanize_config_zh() {
        let err = ClientError::Config("bad base url".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::Zh);
        assert!(msg.contains("客户端配置错误"));
    }

    #[test]
    fn humanize_config_en() {
        let err = ClientError::Config("bad base url".into());
        let msg = humanize_error(&err, ErrorContext::Auth, Language::En);
        assert!(msg.contains("Client configuration error"));
    }

    // ── humanize_error: EmailVerification context ────────

    #[test]
    fn humanize_verify_400_zh() {
        let err = ClientError::Other(400, r#"{"error":"invalid_code"}"#.into());
        let msg = humanize_error(&err, ErrorContext::EmailVerification, Language::Zh);
        assert_eq!(msg, "验证码错误或已过期");
    }

    #[test]
    fn humanize_verify_400_en() {
        let err = ClientError::Other(400, r#"{"error":"invalid_code"}"#.into());
        let msg = humanize_error(&err, ErrorContext::EmailVerification, Language::En);
        assert_eq!(msg, "Invalid or expired verification code");
    }

    #[test]
    fn humanize_verify_503_zh() {
        let err = ClientError::Other(503, r#"{"error":"mail_unconfigured"}"#.into());
        let msg = humanize_error(&err, ErrorContext::EmailVerification, Language::Zh);
        assert_eq!(msg, "邮件服务未配置");
    }

    #[test]
    fn humanize_verify_503_en() {
        let err = ClientError::Other(503, r#"{"error":"mail_unconfigured"}"#.into());
        let msg = humanize_error(&err, ErrorContext::EmailVerification, Language::En);
        assert_eq!(msg, "Email service not configured");
    }

    // ── humanize_error: PasswordReset context ────────────

    #[test]
    fn humanize_reset_400_zh() {
        let err = ClientError::Other(400, r#"{"error":"invalid_token"}"#.into());
        let msg = humanize_error(&err, ErrorContext::PasswordReset, Language::Zh);
        assert_eq!(msg, "重置验证码无效或已过期");
    }

    #[test]
    fn humanize_reset_400_en() {
        let err = ClientError::Other(400, r#"{"error":"invalid_token"}"#.into());
        let msg = humanize_error(&err, ErrorContext::PasswordReset, Language::En);
        assert_eq!(msg, "Invalid or expired reset code");
    }

    #[test]
    fn humanize_reset_503_zh() {
        let err = ClientError::Other(503, r#"{"error":"mail_unconfigured"}"#.into());
        let msg = humanize_error(&err, ErrorContext::PasswordReset, Language::Zh);
        assert_eq!(msg, "密码重置功能暂不可用");
    }

    #[test]
    fn humanize_reset_503_en() {
        let err = ClientError::Other(503, r#"{"error":"mail_unconfigured"}"#.into());
        let msg = humanize_error(&err, ErrorContext::PasswordReset, Language::En);
        assert_eq!(msg, "Password reset is currently unavailable");
    }
}
