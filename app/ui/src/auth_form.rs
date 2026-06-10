use dioxus::prelude::*;

use crate::button::{Button, ButtonType};
use crate::text_input::{InputType, TextInput};

/// AuthForm —— 登录 / 注册 双模表单。
///
/// 按 DESIGN.md §3.10 规格实现：玻璃面板 + 装饰光晕 + Tab 切换。
/// 业务逻辑（`on_submit`）由 web 层注入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthMode {
    #[default]
    Login,
    Register,
}

/// 表单提交时携带的数据。
#[derive(Debug, Clone, PartialEq)]
pub struct AuthPayload {
    pub mode: AuthMode,
    pub name: String,
    pub email: String,
    pub password: String,
    pub remember: bool,
}

#[component]
pub fn AuthForm(
    mode: Signal<AuthMode>,
    name: Signal<String>,
    email: Signal<String>,
    password: Signal<String>,
    #[props(default)] remember: Signal<bool>,
    #[props(default = false)] loading: bool,
    #[props(default)] error: Option<String>,
    on_submit: EventHandler<AuthPayload>,
) -> Element {
    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/auth_form.css"),
        }
        div { class: "ws-auth",
            div { class: "ws-auth__orb ws-auth__orb--purple" }
            div { class: "ws-auth__orb ws-auth__orb--indigo" }

            div { class: "ws-auth__tabs",
                button {
                    r#type: "button",
                    class: if *mode.read() == AuthMode::Login { "ws-auth__tab ws-auth__tab--active" } else { "ws-auth__tab" },
                    onclick: move |_| mode.set(AuthMode::Login),
                    "登录入口 (/login)"
                }
                button {
                    r#type: "button",
                    class: if *mode.read() == AuthMode::Register { "ws-auth__tab ws-auth__tab--active" } else { "ws-auth__tab" },
                    onclick: move |_| mode.set(AuthMode::Register),
                    "注册端口 (/register)"
                }
            }

            form {
                class: "ws-auth__form",
                onsubmit: move |e| {
                    e.prevent_default();
                    on_submit.call(AuthPayload {
                        mode: *mode.read(),
                        name: name.read().clone(),
                        email: email.read().clone(),
                        password: password.read().clone(),
                        remember: *remember.read(),
                    });
                },
                if *mode.read() == AuthMode::Register {
                    TextInput {
                        label: "拟定用户昵称".to_string(),
                        placeholder: Some("e.g., rust_master".to_string()),
                        value: name,
                        required: true,
                        disabled: loading,
                        name: Some("name".to_string()),
                        autocomplete: Some("username".to_string()),
                    }
                }

                TextInput {
                    label: if *mode.read() == AuthMode::Login { "注册绑定的邮箱".to_string() } else { "电子邮箱载体".to_string() },
                    placeholder: Some(if *mode.read() == AuthMode::Login { "name@domain.com".to_string() } else { "master@rust.org".to_string() }),
                    value: email,
                    input_type: InputType::Email,
                    required: true,
                    disabled: loading,
                    name: Some("email".to_string()),
                    autocomplete: Some("email".to_string()),
                }

                TextInput {
                    label: if *mode.read() == AuthMode::Login { "鉴权安全口令".to_string() } else { "强安全密码".to_string() },
                    placeholder: Some("••••••••".to_string()),
                    value: password,
                    input_type: InputType::Password,
                    required: true,
                    disabled: loading,
                    name: Some("password".to_string()),
                    autocomplete: Some(if *mode.read() == AuthMode::Login { "current-password".to_string() } else { "new-password".to_string() }),
                    hint: if *mode.read() == AuthMode::Register {
                        Some("密码需 ≥8 字符，包含大小写字母、数字和 ASCII 标点".to_string())
                    } else {
                        None
                    },
                }

                if let Some(err) = error.as_ref() {
                    p { class: "ws-auth__error", "{err}" }
                }

                if *mode.read() == AuthMode::Login {
                    div { class: "ws-auth__meta",
                        label { class: "ws-auth__remember",
                            input {
                                r#type: "checkbox",
                                checked: *remember.read(),
                                disabled: loading,
                                onchange: move |e| remember.set(e.checked()),
                            }
                            "维持持久化登录"
                        }
                        a { class: "ws-auth__forgot", href: "#", "忘记凭证?" }
                    }
                }

                Button {
                    button_type: ButtonType::Submit,
                    full_width: true,
                    disabled: loading,
                    loading,
                    if *mode.read() == AuthMode::Login {
                        "提交请求验证 [POST /login]"
                    } else {
                        "初始化账户实例 [POST /register]"
                    }
                }
            }
        }
    }
}
