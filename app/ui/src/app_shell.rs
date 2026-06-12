use dioxus::prelude::*;

/// AppShell —— 根布局。
///
/// 按 DESIGN.md §3.1 规格：flex + min-height:100vh，应用 page-gradient 背景。
///
/// 注意：设计令牌 `tokens.css` 由 `ui::GlobalStyles` 在根 App 统一注入，
/// 本组件不再重复加载。消费方必须确保根 App 挂载了 `ui::GlobalStyles {}`。
#[component]
pub fn AppShell(sidebar: Element, top_header: Element, children: Element) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/app_shell.css") }
        div { class: "ws-app-shell",
            {sidebar}
            div { class: "ws-app-shell__main",
                {top_header}
                main { class: "ws-app-shell__content", {children} }
            }
        }
    }
}
