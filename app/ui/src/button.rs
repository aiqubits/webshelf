use dioxus::prelude::*;
use dioxus_icons::lucide::LoaderCircle;

/// 按钮 —— 唯一 CTA 变体（indigo→purple 渐变）。
///
/// 按 DESIGN.md §3.4 规格实现。原系统中**不**提供 secondary / text / icon 变体。
#[component]
pub fn Button(
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
    #[props(default = false)] disabled: bool,
    #[props(default = ButtonType::Button)] button_type: ButtonType,
    #[props(default = false)] full_width: bool,
    #[props(default = false)] loading: bool,
) -> Element {
    let class = if full_width {
        "ws-btn ws-btn--primary ws-btn--block"
    } else {
        "ws-btn ws-btn--primary"
    };

    let type_attr = match button_type {
        ButtonType::Button => "button",
        ButtonType::Submit => "submit",
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/button.css") }
        button {
            class,
            r#type: type_attr,
            disabled: disabled || loading,
            onclick: move |e| {
                if let Some(cb) = onclick {
                    cb.call(e);
                }
            },
            if loading {
                LoaderCircle { class: "ws-btn__spinner" }
            }
            {children}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonType {
    #[default]
    Button,
    Submit,
}
