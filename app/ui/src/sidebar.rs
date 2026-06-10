use dioxus::prelude::*;

use crate::badge::{Badge, BadgeVariant};

/// Sidebar —— 256px 玻璃侧边栏。
///
/// 按 DESIGN.md §3.2 规格。导航条目通过 `NavKey` 抽象，路由映射由 web 层负责。
#[component]
pub fn Sidebar(
    open: Signal<bool>,
    on_close: EventHandler<MouseEvent>,
    active: NavKey,
    on_select: EventHandler<NavKey>,
) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/sidebar.css") }
        // 移动端 overlay
        if open() {
            div {
                class: "ws-sidebar-overlay",
                onclick: move |e| on_close.call(e),
            }
        }
        aside { class: if open() { "ws-sidebar ws-sidebar--open" } else { "ws-sidebar" },
            // Logo 块
            div { class: "ws-sidebar__logo",
                div { class: "ws-sidebar__logo-icon",
                    i { class: "fa-solid fa-layer-group" }
                }
                div { class: "ws-sidebar__logo-text",
                    span { class: "ws-sidebar__logo-title", "WebShelf Admin" }
                    span { class: "ws-sidebar__logo-subtitle", "Rust Fullstack Framework" }
                }
                // 移动端关闭按钮
                button {
                    class: "ws-sidebar__close",
                    onclick: move |e| on_close.call(e),
                    i { class: "fa-solid fa-xmark" }
                }
            }

            // 导航
            nav { class: "ws-sidebar__nav no-scrollbar",
                // 核心系统监控
                div { class: "ws-sidebar__group",
                    div { class: "ws-sidebar__group-caption", "核心系统监控" }
                    SidebarItem {
                        icon: "fa-chart-pie",
                        label: "控制中心 (/health)",
                        active: active == NavKey::Dashboard,
                        onclick: move |_| on_select.call(NavKey::Dashboard),
                    }
                }

                // 数据管理 + admin_layer 徽章
                div { class: "ws-sidebar__group",
                    div { class: "ws-sidebar__group-caption-row",
                        span { class: "ws-sidebar__group-caption", "数据管理" }
                        Badge { variant: BadgeVariant::AmberCompact, "admin_layer" }
                    }
                    SidebarItem {
                        icon: "fa-users-gear",
                        label: "用户管理 (/users)",
                        active: active == NavKey::Users,
                        onclick: move |_| on_select.call(NavKey::Users),
                    }
                }
            }

            // Footer
            div { class: "ws-sidebar__footer",
                a {
                    class: "ws-sidebar__github-card",
                    href: "https://github.com/aiqubits/webshelf",
                    target: "_blank",
                    i { class: "fa-brands fa-github ws-sidebar__github-icon" }
                    div { class: "ws-sidebar__github-text",
                        span { class: "ws-sidebar__github-repo", "aiqubits/webshelf" }
                        span { class: "ws-sidebar__github-sub", "在 GitHub 上查看源码" }
                    }
                    i { class: "fa-solid fa-arrow-up-right-from-square ws-sidebar__github-arrow" }
                }
                div { class: "ws-sidebar__copyright",
                    div { "© 2026 WebShelf Scaffold." }
                    div { "Powered by Rust & Axum" }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Dashboard,
    Users,
}

#[component]
fn SidebarItem(
    icon: &'static str,
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "ws-sidebar__item ws-sidebar__item--active"
    } else {
        "ws-sidebar__item"
    };
    rsx! {
        button { class, onclick,
            i { class: "fa-solid {icon} ws-sidebar__item-icon" }
            span { class: "ws-sidebar__item-label", "{label}" }
        }
    }
}
