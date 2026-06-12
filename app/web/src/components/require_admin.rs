//! RequireAdmin —— admin 路由守卫。
//!
//! 用作 `Route` 枚举的 `#[layout(...)]` —— 只包裹 admin 专属路由（`/users`）。
//! 若用户未登录则跳 `/auth`；若已登录但非 admin 则跳 `/`。
//!
//! 守卫分两层：
//! 1. **渲染时检查**：非 admin 用户不渲染 `Outlet`，杜绝首次渲染的闪烁。
//! 2. **effect 重定向**：`auth.user` 变化时触发 `nav.replace()`，确保路由 URL 同步。

use dioxus::prelude::*;

use crate::Route;
use crate::auth::AuthState;

#[component]
pub fn RequireAdmin() -> Element {
    let auth = use_context::<AuthState>();
    let nav = use_navigator();

    // 渲染时判断 —— 非 admin 不渲染 Outlet，防止受保护页面在 effect 触发前短暂可见。
    let user = auth.user.cloned();
    let authorized = user.as_ref().map(|u| u.is_admin()).unwrap_or(false);

    use_effect(move || {
        let user = auth.user.cloned();
        match user {
            None => {
                let _ = nav.replace(Route::Auth {});
            }
            Some(u) if !u.is_admin() => {
                let _ = nav.replace(Route::Dashboard {});
            }
            _ => {}
        }
    });

    if authorized {
        rsx! { Outlet::<Route> {} }
    } else {
        rsx! {}
    }
}
