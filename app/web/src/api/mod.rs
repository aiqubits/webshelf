//! 客户端 API 工厂与 401 拦截辅助。
//!
//! - `make_client()` 根据编译目标返回合适的 `client_api::Client`。
//! - `is_unauth(err)` 判定一个 `ClientError` 是否表示 token 失效。
//! - `handle_unauth(err, auth, nav, log_bus)` 检测到 401 时执行 logout + 跳转 `/auth`
//!   并写入 `LogKind::Important` 日志，与 `TokenExpiryGuard::fire_expiry` 行为对齐。
//! - `humanize_error(err, ctx)` 将 API 错误翻译为中文提示。

mod client;

pub use client::make_client;

use client_api::ClientError;
use dioxus::prelude::dioxus_router::Navigator;

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

/// 将 `ClientError` 翻译为中文提示，根据 `ctx` 差异化状态码文案。
pub fn humanize_error(err: &ClientError, ctx: ErrorContext) -> String {
    match err {
        ClientError::Network(msg) => format!("网络异常: {msg}"),
        ClientError::ServerError(status, body) => {
            format!("服务器错误 (HTTP {status}): {body}")
        }
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
            match ctx {
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
                    // 400 是验证/重发接口的主错误码。
                    // 服务端 anti-enumeration 把 "用户不存在"、"码错误"、"码过期"、
                    // "超过尝试上限" 全部映射到同一文案 ("Invalid or expired
                    // verification code")。前端也保持一致：不对用户暴露区分。
                    // 表单级 validation_error（如 code 长度不对）同样返回 400，
                    // 统一归入此臂——客户端表单校验已在前置拦截，此处兜底即可。
                    (400, _) => "验证码错误或已过期".to_string(),
                    (503, _) => "邮件服务未配置".to_string(),
                    _ => format!("请求失败 (HTTP {status}): {msg}"),
                },
                ErrorContext::PasswordReset => match (status, code.as_str()) {
                    // forgot-password: 服务端对未知邮箱与 cooldown 早请求均 200 兜底，
                    // 真实错误只会是 503（邮件服务未配置）。
                    // reset-password: 服务端把 "token 不存在 / 已过期 / 错误 /
                    // 已被消费 / 暴力尝试上限 / 弱密码" 全部统一 400 + 通用文案，
                    // 前端保持同样的反枚举语义。
                    (400, _) => "重置验证码无效或已过期".to_string(),
                    (503, _) => "密码重置功能暂不可用".to_string(),
                    _ => format!("请求失败 (HTTP {status}): {msg}"),
                },
            }
        }
        ClientError::RateLimited(_) => "请求过于频繁，请稍后再试".to_string(),
        ClientError::Deserialization(msg) => format!("响应解析失败: {msg}"),
        ClientError::Config(msg) => format!("客户端配置错误: {msg}"),
        _ => format!("未知错误: {err}"),
    }
}

/// 判定一个 `ClientError` 是否代表 token 失效（HTTP 401）。
pub fn is_unauth(err: &ClientError) -> bool {
    matches!(err, ClientError::Other(401, _))
}

/// 若 `err` 为 401 或 403，执行相应导航并返回 `true`：
/// - 401：注销并跳 `/auth`（token 过期/无效）；
/// - 403：仅跳 `/`（已认证但权限不足，如 JWT role 被篡改）。
///
/// 否则返回 `false`，让调用方继续处理业务错误。
///
/// 视图层模式：
/// ```ignore
/// if let Err(e) = client.list_users(1, 20).await {
///     if handle_unauth(&e, auth, nav, log_bus) { return; }
///     // 处理业务错误...
/// }
/// ```
pub fn handle_unauth(
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
        auth.logout();
        nav.replace(Route::Auth {});
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
    fn humanize_auth_401() {
        let err = ClientError::Other(401, r#"{"error":"unauthorized"}"#.into());
        let msg = humanize_error(&err, ErrorContext::Auth);
        assert_eq!(msg, "邮箱或密码错误");
    }

    #[test]
    fn humanize_auth_validation_error() {
        let err = ClientError::Other(
            400,
            r#"{"error":"validation_error","message":"email is invalid"}"#.into(),
        );
        let msg = humanize_error(&err, ErrorContext::Auth);
        assert!(msg.contains("参数错误"));
    }

    #[test]
    fn humanize_auth_conflict() {
        let err = ClientError::Other(
            409,
            r#"{"error":"conflict","message":"email already registered"}"#.into(),
        );
        let msg = humanize_error(&err, ErrorContext::Auth);
        assert_eq!(msg, "该邮箱已注册");
    }

    #[test]
    fn humanize_auth_network() {
        let err = ClientError::Network("connection refused".into());
        let msg = humanize_error(&err, ErrorContext::Auth);
        assert!(msg.contains("网络异常"));
    }

    // ── humanize_error: UserManagement context ───────────

    #[test]
    fn humanize_usermgmt_401() {
        let err = ClientError::Other(401, r#"{"error":"unauthorized"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement);
        assert_eq!(msg, "未登录或会话已过期");
    }

    #[test]
    fn humanize_usermgmt_403() {
        let err = ClientError::Other(403, r#"{"error":"forbidden"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement);
        assert_eq!(msg, "权限不足 (需 admin)");
    }

    #[test]
    fn humanize_usermgmt_404() {
        let err = ClientError::Other(404, r#"{"error":"not_found"}"#.into());
        let msg = humanize_error(&err, ErrorContext::UserManagement);
        assert_eq!(msg, "用户不存在");
    }

    #[test]
    fn humanize_usermgmt_server_error() {
        let err = ClientError::ServerError(500, "Internal Server Error".into());
        let msg = humanize_error(&err, ErrorContext::UserManagement);
        assert!(msg.contains("服务器错误"));
    }

    #[test]
    fn humanize_rate_limited() {
        let err = ClientError::RateLimited("Too many requests".into());
        let msg = humanize_error(&err, ErrorContext::Auth);
        assert_eq!(msg, "请求过于频繁，请稍后再试");
    }

    #[test]
    fn humanize_deserialization() {
        let err = ClientError::Deserialization("expected `{`".into());
        let msg = humanize_error(&err, ErrorContext::Auth);
        assert!(msg.contains("响应解析失败"));
    }

    #[test]
    fn humanize_config() {
        let err = ClientError::Config("bad base url".into());
        let msg = humanize_error(&err, ErrorContext::Auth);
        assert!(msg.contains("客户端配置错误"));
    }

    // ── humanize_error: EmailVerification context ────────

    #[test]
    fn humanize_verify_400() {
        let err = ClientError::Other(400, r#"{"error":"invalid_code"}"#.into());
        let msg = humanize_error(&err, ErrorContext::EmailVerification);
        assert_eq!(msg, "验证码错误或已过期");
    }

    #[test]
    fn humanize_verify_503() {
        let err = ClientError::Other(503, r#"{"error":"mail_unconfigured"}"#.into());
        let msg = humanize_error(&err, ErrorContext::EmailVerification);
        assert_eq!(msg, "邮件服务未配置");
    }

    // ── humanize_error: PasswordReset context ────────────

    #[test]
    fn humanize_reset_400() {
        let err = ClientError::Other(400, r#"{"error":"invalid_token"}"#.into());
        let msg = humanize_error(&err, ErrorContext::PasswordReset);
        assert_eq!(msg, "重置验证码无效或已过期");
    }

    #[test]
    fn humanize_reset_503() {
        let err = ClientError::Other(503, r#"{"error":"mail_unconfigured"}"#.into());
        let msg = humanize_error(&err, ErrorContext::PasswordReset);
        assert_eq!(msg, "密码重置功能暂不可用");
    }
}
