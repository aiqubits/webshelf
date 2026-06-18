use dioxus::prelude::*;
use dioxus_icons::lucide::{ChevronDown, LogOut, Menu, Search, Settings};

/// TopHeader —— 80px sticky 顶栏。
///
/// 按 DESIGN.md §3.3 规格。
/// 用户区域点击后弹出下拉菜单，包含「个人设置」和「登出」两项。
#[component]
pub fn TopHeader(
    on_sidebar_toggle: EventHandler<MouseEvent>,
    search_value: Signal<String>,
    user_name: String,
    user_email: String,
    /// 点击「个人设置」时触发，通常用于跳转到设置页。
    #[props(default)]
    on_settings_click: Option<EventHandler<MouseEvent>>,
    /// 点击「登出」时触发，通常由调用方弹出确认框后执行登出。
    #[props(default)]
    on_logout: Option<EventHandler<MouseEvent>>,
) -> Element {
    let mut dropdown_open = use_signal(|| false);

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/top_header.css") }
        header { class: "ws-top-header",
            // 左侧：汉堡菜单 + 搜索框
            div { class: "ws-top-header__left",
                button {
                    class: "ws-top-header__hamburger",
                    onclick: move |e| on_sidebar_toggle.call(e),
                    Menu {}
                }
                div { class: "ws-top-header__search",
                    Search { class: "ws-top-header__search-icon" }
                    input {
                        class: "ws-top-header__search-input",
                        r#type: "text",
                        placeholder: "搜索资源与 API 端点...",
                        value: search_value,
                        oninput: move |e| *search_value.write() = e.value(),
                    }
                }
            }

            // 右侧：状态 + 用户下拉菜单
            div { class: "ws-top-header__right",
                span { class: "ws-top-header__status",
                    span { class: "ws-top-header__status-dot" }
                    span { class: "ws-top-header__status-text", "Node Online" }
                }

                // 用户下拉菜单容器
                div { class: "ws-top-header__user-menu",
                    // 触发区：头像 + 身份
                    div {
                        class: "ws-top-header__user ws-top-header__user--clickable",
                        title: "点击展开菜单",
                        onclick: move |_| dropdown_open.toggle(),
                        div { class: "ws-top-header__avatar", "WS" }
                        div { class: "ws-top-header__identity",
                            span { class: "ws-top-header__name", "{user_name}" }
                            span { class: "ws-top-header__email", "{user_email}" }
                        }
                        ChevronDown { class: "ws-top-header__user-chevron" }
                    }

                    // 下拉菜单
                    if dropdown_open() {
                        // 全屏透明遮罩 —— 点击非菜单区域关闭下拉菜单
                        div {
                            class: "ws-top-header__dropdown-overlay",
                            onclick: move |_| dropdown_open.set(false),
                        }
                        div { class: "ws-top-header__dropdown",
                            // 菜单项：个人设置
                            button {
                                class: "ws-top-header__dropdown-item",
                                r#type: "button",
                                onclick: move |e| {
                                    dropdown_open.set(false);
                                    if let Some(ref h) = on_settings_click {
                                        h.call(e);
                                    }
                                },
                                Settings { class: "ws-top-header__dropdown-icon" }
                                span { "个人设置" }
                            }
                            // 分隔线
                            div { class: "ws-top-header__dropdown-divider" }
                            // 菜单项：登出
                            button {
                                class: "ws-top-header__dropdown-item ws-top-header__dropdown-item--danger",
                                r#type: "button",
                                onclick: move |e| {
                                    dropdown_open.set(false);
                                    if let Some(ref h) = on_logout {
                                        h.call(e);
                                    }
                                },
                                LogOut { class: "ws-top-header__dropdown-icon" }
                                span { "登出" }
                            }
                        }
                    }
                }
            }
        }
    }
}
