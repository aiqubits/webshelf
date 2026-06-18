use dioxus::prelude::*;
use dioxus_icons::lucide::{ChartPie, ExternalLink, Layers, UserCog, X};

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
                div { class: "ws-sidebar__logo-icon", Layers {} }
                div { class: "ws-sidebar__logo-text",
                    span { class: "ws-sidebar__logo-title", "WebShelf Admin" }
                    span { class: "ws-sidebar__logo-subtitle", "Rust Fullstack" }
                }
                // 移动端关闭按钮
                button {
                    class: "ws-sidebar__close",
                    onclick: move |e| on_close.call(e),
                    X {}
                }
            }

            // 导航
            nav { class: "ws-sidebar__nav no-scrollbar",
                // 系统监控
                div { class: "ws-sidebar__group",
                    div { class: "ws-sidebar__group-caption", "系统监控" }
                    SidebarItem {
                        icon: rsx! {
                            ChartPie {}
                        },
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
                        icon: rsx! {
                            UserCog {}
                        },
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
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "currentColor",
                        class: "ws-sidebar__github-icon",
                        path { d: "M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" }
                    }
                    div { class: "ws-sidebar__github-text",
                        span { class: "ws-sidebar__github-repo", "aiqubits/webshelf" }
                        span { class: "ws-sidebar__github-sub", "在 GitHub 上查看源码" }
                    }
                    ExternalLink { class: "ws-sidebar__github-arrow" }
                }
                div { class: "ws-sidebar__copyright",
                    div { "© 2026 WebShelf Fullstack Framework." }
                    div { "Powered by WebShelf" }
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
    icon: Element,
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
            {icon}
            span { class: "ws-sidebar__item-label", "{label}" }
        }
    }
}
