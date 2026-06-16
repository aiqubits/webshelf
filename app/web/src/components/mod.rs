//! Web 应用专属组件 / 上下文提供者。
//!
//! - `AppShellLayout`：将 `AppShell + Sidebar + TopHeader + Outlet<Route>` 装配在一起。
//! - `RequireAdmin`：admin 路由守卫（layout）。
//! - `TokenExpiryGuard`：JWT 过期自动登出。
//! - `LogBus`：toast + console 共享事件总线。

mod app_shell_layout;
mod log_bus;
mod require_admin;
mod token_expiry_guard;

pub use app_shell_layout::{AppShellLayout, SearchSignal};
pub use log_bus::{
    HttpMethod, LogBus, LogEntry, LogKind, now_unix_ms, now_unix_secs, push_log_err, push_log_ok,
    push_log_result,
};
pub use require_admin::RequireAdmin;
pub use token_expiry_guard::TokenExpiryGuard;
