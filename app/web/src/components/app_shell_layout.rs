use dioxus::prelude::*;

use ui::{AppShell, NavKey, Sidebar, TopHeader};

/// 搜索信号包装类型，通过 `use_context_provider` 全局注入。
///
/// 在 `AppShellLayout` 中创建，`Users` 等消费方通过 `use_context` 读取。
#[derive(Clone, Copy)]
pub struct SearchSignal(pub Signal<String>);

use crate::Route;
use crate::auth::AuthState;
use crate::components::{ConfirmDialog, TokenExpiryGuard};

/// 将 `AppShell` + `Sidebar` + `TopHeader` + `Outlet<Route>` 装配在一起的 web 专用布局。
///
/// 持有移动端侧边栏抽屉状态 (`sidebar_open`)，并从 `AuthState` 读取当前用户身份
/// 注入到 `TopHeader`。
#[component]
pub fn AppShellLayout() -> Element {
    let mut sidebar_open = use_signal(|| false);
    let search_value = use_signal(String::new);
    use_context_provider(|| SearchSignal(search_value));
    let nav = use_navigator();
    let auth = use_context::<AuthState>();

    // 登出确认弹窗状态
    let mut show_logout_confirm = use_signal(|| false);

    let route = use_route::<Route>();
    let active_nav = match route {
        Route::Dashboard {} => NavKey::Dashboard,
        Route::Users {} => NavKey::Users,
        // Settings 在本布局内但没有专用 NavKey，沿用 Dashboard 高亮。
        Route::Settings {} => NavKey::Dashboard,
        // 通配臂——此布局仅包裹 Dashboard / Settings / Users（见 main.rs 路由定义），
        // 其他 Route 变体不应到达本布局。若未来新增路由加入此布局，
        // 编译器不会警告，需手动在此处补充分支。
        _ => NavKey::Dashboard,
    };

    // 从 AuthState 派生展示用身份信息
    let (user_name, user_email) = match auth.user.read().as_ref() {
        Some(u) => (u.name.clone(), u.email.clone()),
        None => ("Guest".to_string(), "未登录".to_string()),
    };
    // 检查当前用户角色，控制 admin 模块的可见性
    let is_admin = auth
        .user
        .read()
        .as_ref()
        .map(|u| u.is_admin())
        .unwrap_or(false);

    rsx! {
        AppShell {
            sidebar: rsx! {
                Sidebar {
                    open: sidebar_open,
                    on_close: move |_| sidebar_open.set(false),
                    active: active_nav,
                    show_admin_modules: is_admin,
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
                    search_value,
                    user_name,
                    user_email,
                    on_settings_click: move |_| {
                        nav.push(Route::Settings {});
                    },
                    on_logout: move |_| {
                        show_logout_confirm.set(true);
                    },
                }
            },
            Outlet::<Route> {}
            TokenExpiryGuard {}

            // 登出确认弹窗
            ConfirmDialog {
                open: *show_logout_confirm.read(),
                title: "确认登出".to_string(),
                message: "确定要退出当前账号吗？".to_string(),
                danger: true,
                on_confirm: move |_| {
                    let mut auth_async = auth.clone();
                    let nav_async = nav;
                    spawn(async move {
                        auth_async.logout_async().await;
                        nav_async.replace(Route::LoginLanding {});
                    });
                    show_logout_confirm.set(false);
                },
                on_cancel: move |_| show_logout_confirm.set(false),
            }
        }
    }
}
