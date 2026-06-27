use dioxus::prelude::*;

use crate::button::{Button, ButtonType};
use crate::text_input::{InputType, TextInput};
use crate::{EN, I18nContext};

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
    pub password_confirm: String,
    pub remember: bool,
}

#[component]
pub fn AuthForm(
    mode: Signal<AuthMode>,
    name: Signal<String>,
    email: Signal<String>,
    password: Signal<String>,
    password_confirm: Signal<String>,
    #[props(default)] remember: Option<Signal<bool>>,
    #[props(default = false)] loading: bool,
    #[props(default)] error: Option<String>,
    /// 点击「忘记凭证?」链接时由调用方注入的处理（如导航到密码重置路由）。
    /// 不传则降级为默认行为（链接不可点击）。
    #[props(default)]
    on_forgot: Option<EventHandler<MouseEvent>>,
    on_submit: EventHandler<AuthPayload>,
) -> Element {
    let i18n = try_use_context::<I18nContext>();
    let t = i18n.as_ref().map(|c| c.t()).unwrap_or(&EN);

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/auth_form.css") }
        div { class: "ws-auth",
            div { class: "ws-auth__orb ws-auth__orb--purple" }
            div { class: "ws-auth__orb ws-auth__orb--indigo" }

            div { class: "ws-auth__tabs",
                button {
                    r#type: "button",
                    class: if *mode.read() == AuthMode::Login { "ws-auth__tab ws-auth__tab--active" } else { "ws-auth__tab" },
                    onclick: move |_| mode.set(AuthMode::Login),
                    "{t.auth_login_tab} (/login)"
                }
                button {
                    r#type: "button",
                    class: if *mode.read() == AuthMode::Register { "ws-auth__tab ws-auth__tab--active" } else { "ws-auth__tab" },
                    onclick: move |_| mode.set(AuthMode::Register),
                    "{t.auth_register_tab} (/register)"
                }
            }

            form {
                class: "ws-auth__form",
                novalidate: true,
                onsubmit: move |e| {
                    e.prevent_default();
                    on_submit
                        .call(AuthPayload {
                            mode: *mode.read(),
                            name: name.read().clone(),
                            email: email.read().clone(),
                            password: password.read().clone(),
                            password_confirm: password_confirm.read().clone(),
                            remember: remember.as_ref().map(|s| *s.read()).unwrap_or(false),
                        });
                },
                if *mode.read() == AuthMode::Register {
                    TextInput {
                        label: t.auth_name_label.to_string(),
                        placeholder: Some(t.auth_name_placeholder.to_string()),
                        value: name,
                        required: true,
                        disabled: loading,
                        name: Some("name".to_string()),
                        autocomplete: Some("username".to_string()),
                    }
                }

                TextInput {
                    label: if *mode.read() == AuthMode::Login { t.auth_email_label_login.to_string() } else { t.auth_email_label_register.to_string() },
                    placeholder: Some(
                        if *mode.read() == AuthMode::Login {
                            t.auth_email_placeholder_login.to_string()
                        } else {
                            t.auth_email_placeholder_register.to_string()
                        },
                    ),
                    value: email,
                    input_type: InputType::Email,
                    required: true,
                    disabled: loading,
                    name: Some("email".to_string()),
                    autocomplete: Some("email".to_string()),
                }

                TextInput {
                    label: if *mode.read() == AuthMode::Login { t.auth_password_label_login.to_string() } else { t.auth_password_label_register.to_string() },
                    placeholder: Some(t.auth_password_placeholder.to_string()),
                    value: password,
                    input_type: InputType::Password,
                    required: true,
                    disabled: loading,
                    name: Some("password".to_string()),
                    autocomplete: Some(
                        if *mode.read() == AuthMode::Login {
                            "current-password".to_string()
                        } else {
                            "new-password".to_string()
                        },
                    ),
                    hint: if *mode.read() == AuthMode::Register { Some(
                        t.auth_password_hint.to_string(),
                    ) } else { None },
                }

                if *mode.read() == AuthMode::Register {
                    TextInput {
                        label: t.auth_password_confirm_label.to_string(),
                        placeholder: Some(t.auth_password_placeholder.to_string()),
                        value: password_confirm,
                        input_type: InputType::Password,
                        required: true,
                        disabled: loading,
                        name: Some("password_confirm".to_string()),
                        autocomplete: Some("new-password".to_string()),
                    }
                }

                if let Some(err) = error.as_ref() {
                    p { class: "ws-auth__error", "{err}" }
                }

                if *mode.read() == AuthMode::Login {
                    div { class: "ws-auth__meta",
                        label { class: "ws-auth__remember",
                            input {
                                r#type: "checkbox",
                                checked: remember.as_ref().map(|s| *s.read()).unwrap_or(false),
                                disabled: loading,
                                onchange: move |e| {
                                    if let Some(ref mut r) = remember {
                                        r.set(e.checked());
                                    }
                                },
                            }
                            {t.auth_remember_label}
                        }
                        // 「忘记凭证?」链接 —— 旧版硬编码 `href="#"` 是死链；
                        // 现由调用方注入 on_forgot 实现真正的导航，未传则降级为不可点击。
                        a {
                            class: "ws-auth__forgot",
                            href: "#",
                            onclick: move |e| {
                                if let Some(ref h) = on_forgot {
                                    e.prevent_default();
                                    h.call(e);
                                }
                            },
                            {t.auth_forgot_label}
                        }
                    }
                }

                Button {
                    button_type: ButtonType::Submit,
                    full_width: true,
                    disabled: loading,
                    loading,
                    if *mode.read() == AuthMode::Login {
                        "{t.auth_submit_login} [POST /login]"
                    } else {
                        "{t.auth_submit_register} [POST /register]"
                    }
                }
            }
        }
    }
}
