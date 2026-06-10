//! RequireAdmin —— admin 路由守卫。
//!
//! 用作 `Route` 枚举的 `#[layout(...)]` —— 只包裹 admin 专属路由（`/users`）。
//! 若用户未登录则跳 `/auth`；若已登录但非 admin 则跳 `/`。
//! guard 是 effect-based：每次 `auth.user` 变化或首次挂载时检查，
//! 避免重定向风暴。

use dioxus::prelude::*;

use crate::Route;
use crate::auth::AuthState;

#[component]
pub fn RequireAdmin() -> Element {
    let auth = use_context::<AuthState>();
    let nav = use_navigator();

    use_effect(move || {
        let user = auth.user.cloned();
        match user {
            None => {
                let _ = nav.push(Route::Auth {});
            }
            Some(u) if !u.is_admin() => {
                let _ = nav.push(Route::Dashboard {});
            }
            _ => {}
        }
    });

    rsx! { Outlet::<Route> {} }
}
