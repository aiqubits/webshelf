use dioxus::prelude::*;
use dioxus_icons::lucide::{LogOut, Menu, Search};

/// TopHeader —— 80px sticky 顶栏。
///
/// 按 DESIGN.md §3.3 规格。
#[component]
pub fn TopHeader(
    on_sidebar_toggle: EventHandler<MouseEvent>,
    search_value: Signal<String>,
    user_name: String,
    user_email: String,
    #[props(default)] on_logout: Option<EventHandler<MouseEvent>>,
    #[props(default)] on_user_click: Option<EventHandler<MouseEvent>>,
) -> Element {
    let user_class = if on_user_click.is_some() {
        "ws-top-header__user ws-top-header__user--clickable"
    } else {
        "ws-top-header__user"
    };
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

            // 右侧：状态 + 分隔线 + 头像 + 身份 + 登出
            div { class: "ws-top-header__right",
                span { class: "ws-top-header__status",
                    span { class: "ws-top-header__status-dot" }
                    span { class: "ws-top-header__status-text", "Axum Node Online" }
                }
                span { class: "ws-top-header__divider" }
                div {
                    class: user_class,
                    title: if on_user_click.is_some() { "点击进入个人设置" } else { "" },
                    onclick: move |e| {
                        if let Some(ref h) = on_user_click {
                            h.call(e);
                        }
                    },
                    div { class: "ws-top-header__avatar", "WS" }
                    div { class: "ws-top-header__identity",
                        span { class: "ws-top-header__name", "{user_name}" }
                        span { class: "ws-top-header__email", "{user_email}" }
                    }
                }
                match on_logout {
                    Some(handler) => rsx! {
                        span { class: "ws-top-header__divider" }
                        button {
                            class: "ws-top-header__logout",
                            r#type: "button",
                            title: "登出",
                            onclick: move |e| handler.call(e),
                            LogOut {}
                            span { "登出" }
                        }
                    },
                    None => rsx! {},
                }
            }
        }
    }
}
