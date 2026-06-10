use dioxus::prelude::*;

use ui::{AppShell, NavKey, Sidebar, TopHeader};

use crate::Route;
use crate::auth::AuthState;
use crate::components::TokenExpiryGuard;

/// 将 `AppShell` + `Sidebar` + `TopHeader` + `Outlet<Route>` 装配在一起的 web 专用布局。
///
/// 持有移动端侧边栏抽屉状态 (`sidebar_open`)，并从 `AuthState` 读取当前用户身份
/// 注入到 `TopHeader`。
#[component]
pub fn AppShellLayout() -> Element {
    let mut sidebar_open = use_signal(|| false);
    let search_value = use_signal(String::new);
    let nav = use_navigator();
    let mut auth = use_context::<AuthState>();

    let route = use_route::<Route>();
    let active_nav = match route {
        Route::Dashboard {} => NavKey::Dashboard,
        Route::Users {} => NavKey::Users,
        // 本布局只包裹 Dashboard 和 Users，其他路由不会进到这里。
        _ => NavKey::Dashboard,
    };

    // 从 AuthState 派生展示用身份信息
    let (user_name, user_email) = match auth.user.read().as_ref() {
        Some(u) => (u.name.clone(), u.email.clone()),
        None => ("Guest".to_string(), "未登录".to_string()),
    };

    rsx! {
        AppShell {
            sidebar: rsx! {
                Sidebar {
                    open: sidebar_open,
                    on_close: move |_| sidebar_open.set(false),
                    active: active_nav,
                    on_select: move |key| {
                        let target = match key {
                            NavKey::Dashboard => Route::Dashboard {},
                            NavKey::Users => Route::Users {},
                        };
                        sidebar_open.set(false);
                        nav.push(target);
                    },
                }
            },
            top_header: rsx! {
                TopHeader {
                    on_sidebar_toggle: move |_| sidebar_open.toggle(),
                    search_value: search_value,
                    user_name: user_name,
                    user_email: user_email,
                    on_logout: move |_| {
                        auth.logout();
                        nav.push(Route::Auth {});
                    },
                }
            },
            Outlet::<Route> {}
            TokenExpiryGuard {}
        }
    }
}
