use dioxus::prelude::*;
use dioxus_icons::lucide::X;

use crate::{EN, I18nContext};

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
    #[props(default = false)] disable_backdrop: bool,
    children: Element,
) -> Element {
    let i18n = try_use_context::<I18nContext>();
    let t = i18n.as_ref().map(|c| c.t()).unwrap_or(&EN);

    if !open {
        return rsx! {};
    }

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/modal.css") }
        div {
            class: "ws-modal__backdrop",
            onclick: move |e| {
                if !disable_backdrop {
                    on_close.call(e);
                }
            },
        }
        div { class: "ws-modal__wrap", role: "dialog", aria_modal: "true",
            div { class: "ws-modal__card",
                header { class: "ws-modal__header",
                    h3 { class: "ws-modal__title", "{title}" }
                    button {
                        class: "ws-modal__close",
                        r#type: "button",
                        aria_label: t.modal_close,
                        onclick: move |e| on_close.call(e),
                        X {}
                    }
                }
                div { class: "ws-modal__body", {children} }
            }
        }
    }
}
