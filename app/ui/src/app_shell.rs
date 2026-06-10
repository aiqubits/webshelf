use dioxus::prelude::*;

/// AppShell —— 根布局。
///
/// 按 DESIGN.md §3.1 规格：flex + min-height:100vh，应用 page-gradient 背景。
/// 同时加载 `tokens.css`，使所有消费方自动获得设计令牌 CSS 变量。
#[component]
pub fn AppShell(sidebar: Element, top_header: Element, children: Element) -> Element {
    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/tokens.css"),
        }
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/app_shell.css"),
        }
        div { class: "ws-app-shell",
            {sidebar}
            div { class: "ws-app-shell__main",
                {top_header}
                main { class: "ws-app-shell__content", {children} }
            }
        }
    }
}
