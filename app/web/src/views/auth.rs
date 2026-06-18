//! Auth 视图 —— 兼容旧路由 `/auth`。
//!
//! 直接重定向到 `LoginLanding`（`/`），不再包含冗余的登录/注册表单逻辑。
//! 登录功能已统一由 `LoginLanding` 提供。

use dioxus::prelude::*;

use crate::Route;
use crate::auth::AuthState;

/// Auth 视图 —— 兼容旧路由 `/auth`。
///
/// 直接重定向到 `LoginLanding`（`/`），不再包含冗余的登录/注册表单逻辑。
/// 登录功能已统一由 `LoginLanding` 提供。
#[component]
pub fn Auth() -> Element {
    let nav = use_navigator();
    let auth = use_context::<AuthState>();

    // 等待 AuthState 初始化完成，避免记住登录用户首屏被误判为未登录
    if !*auth.initialized.read() {
        return rsx! {
            Fragment {}
        };
    }

    use_effect(move || {
        // 在 effect 内部读取最新状态，确保 initialized 变化时能正确触发重定向
        if *auth.initialized.read() {
            if auth.is_authenticated() {
                nav.replace(Route::Dashboard {});
            } else {
                nav.replace(Route::LoginLanding {});
            }
        }
    });

    rsx! {
        Fragment {}

    }
}
