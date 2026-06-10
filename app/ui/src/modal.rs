use dioxus::prelude::*;

/// 通用 Modal 容器 —— 按 DESIGN.md §3.7 规格实现。
///
/// 用法：
/// ```ignore
/// Modal { title: "创建新用户", on_close, open: true,
///     TextInput { label: "账户", value, on_input }
/// }
/// ```
#[component]
pub fn Modal(
    title: String,
    on_close: EventHandler<MouseEvent>,
    #[props(default = true)] open: bool,
    children: Element,
) -> Element {
    if !open {
        return rsx! {};
    }

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/modal.css"),
        }
        div {
            class: "ws-modal__backdrop",
            onclick: move |e| on_close.call(e),
        }
        div { class: "ws-modal__wrap", role: "dialog", aria_modal: "true",
            div { class: "ws-modal__card",
                header { class: "ws-modal__header",
                    h3 { class: "ws-modal__title", "{title}" }
                    button {
                        class: "ws-modal__close",
                        r#type: "button",
                        aria_label: "关闭",
                        onclick: move |e| on_close.call(e),
                        i { class: "fa-solid fa-xmark" }
                    }
                }
                div { class: "ws-modal__body", {children} }
            }
        }
    }
}
