//! RequireAuth —— 认证路由守卫。
//!
//! 用作 `Route` 枚举的 `#[layout(...)]` —— 包裹所有需要登录才能访问的路由。
//! 若用户未登录则重定向到 `/`（登录入口页）。
//!
//! 守卫分两层：
//! 1. **渲染时检查**：未登录用户不渲染 `Outlet`，杜绝首次渲染的闪烁。
//! 2. **effect 重定向**：`auth.user` 变化时触发 `nav.replace()`，确保路由 URL 同步。
//!
//! 注意：必须同时检查 `initialized` 和 `authenticated`，避免 AuthState 尚未从
//! cookie 恢复会话时（`restore_from_storage_async` 进行中）误判为未登录，
//! 导致「记住登录」用户首屏被踢到登录页再跳回的闪烁问题。

use dioxus::prelude::*;

use crate::Route;
use crate::auth::AuthState;

#[component]
pub fn RequireAuth() -> Element {
    let auth = use_context::<AuthState>();
    let nav = use_navigator();

    // 读取当前快照 —— 必须同时检查 initialized 和 authenticated
    let is_initialized = *auth.initialized.read();
    let is_authenticated = auth.is_authenticated();

    use_effect(move || {
        // 仅在初始化完成且未登录时才重定向，防止「记住登录」用户首屏被误踢
        if *auth.initialized.read() && !auth.is_authenticated() {
            let _ = nav.replace(Route::LoginLanding {});
        }
    });

    if is_initialized && is_authenticated {
        rsx! {
            Outlet::<Route> {}
        }
    } else {
        rsx! {
            Fragment {}
        }
    }
}
