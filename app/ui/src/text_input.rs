use dioxus::prelude::*;
use dioxus_icons::lucide::{Eye, EyeOff};

/// TextInput —— 表单输入框。
///
/// 按 DESIGN.md §3.6 规格。`dense: true` 切换为 Modal 内的紧凑变体。
#[component]
pub fn TextInput(
    value: Signal<String>,
    #[props(default)] label: Option<String>,
    #[props(default)] placeholder: Option<String>,
    #[props(default = InputType::Text)] input_type: InputType,
    #[props(default = false)] required: bool,
    #[props(default = false)] dense: bool,
    #[props(default)] error: Option<String>,
    #[props(default)] hint: Option<String>,
    #[props(default = false)] disabled: bool,
    #[props(default)] name: Option<String>,
    #[props(default)] autocomplete: Option<String>,
) -> Element {
    let mut show_password = use_signal(|| false);

    let input_class = if dense {
        "ws-input__field ws-input__field--dense"
    } else {
        "ws-input__field"
    };
    let type_attr = if input_type == InputType::Password && *show_password.read() {
        "text"
    } else {
        match input_type {
            InputType::Text => "text",
            InputType::Email => "email",
            InputType::Password => "password",
            InputType::Number => "number",
        }
    };

    let field_class = if input_type == InputType::Password {
        format!("{} ws-input__field--pw", input_class)
    } else {
        input_class.to_string()
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/text_input.css") }
        div { class: "ws-input",
            if let Some(label_text) = label.as_ref() {
                label { class: "ws-input__label", "{label_text}" }
            }
            div { class: "ws-input__field-wrapper",
                input {
                    class: "{field_class}",
                    r#type: type_attr,
                    value,
                    required,
                    disabled,
                    placeholder: placeholder.unwrap_or_default(),
                    name: name.unwrap_or_default(),
                    autocomplete: autocomplete.unwrap_or_default(),
                    oninput: move |e| {
                        *value.write() = e.value();
                    },
                }
                if input_type == InputType::Password {
                    button {
                        class: "ws-input__toggle-pw",
                        r#type: "button",
                        tabindex: "-1",
                        aria_label: if *show_password.read() { "Hide password" } else { "Show password" },
                        onclick: move |_| {
                            let current = *show_password.read();
                            *show_password.write() = !current;
                        },
                        if *show_password.read() {
                            Eye { width: "18", height: "18" }
                        } else {
                            EyeOff { width: "18", height: "18" }
                        }
                    }
                }
            }
            if let Some(err) = error.as_ref() {
                p { class: "ws-input__error", "{err}" }
            } else if let Some(hint_text) = hint.as_ref() {
                p { class: "ws-input__hint", "{hint_text}" }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputType {
    #[default]
    Text,
    Email,
    Password,
    Number,
}
