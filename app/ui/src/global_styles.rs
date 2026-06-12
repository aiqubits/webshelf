use dioxus::prelude::*;

/// 全局设计令牌样式注入。
///
/// 仅负责加载 `tokens.css`（CSS 自定义属性集中处），用于**所有路由**——
/// 包括不走 `AppShell` 的页面（如 `Auth`）。其它布局/原语类样式仍由
/// 各自组件就近加载。
///
/// 该组件挂在 web crate 的 `App` 根节点上，确保 tokens 在任意路由切换前已生效。
#[component]
pub fn GlobalStyles() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/tokens.css") }
    }
}
