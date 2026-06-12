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
