//! 忘记密码视图 —— `/forgot-password`。
//!
//! 流程：填写邮箱 → 提交到 `POST /api/public/auth/forgot-password` →
//! 显示通用提示文案（服务端 anti-enumeration，对未知邮箱也返回 200）。
//! 已登录用户访问此路由会被踢回 `/`（改密应走 `/settings`）。

use dioxus::prelude::*;

use ui::{Button, ButtonType, I18nContext, InputType, TextInput};

use crate::Route;
use crate::api::{ErrorContext, humanize_error};
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, push_log_result};

#[component]
pub fn ForgotPassword() -> Element {
    let i18n = use_context::<I18nContext>();
    let t = i18n.t();
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    // ── 钩子必须无条件调用 ──────────────────────────────
    let email = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);

    // 已登录守卫：让"找回密码"对已登录用户毫无意义，直接踢回首页。
    // 注意：必须先等待 initialization 完成（restore_from_storage_async），
    // 否则 authenticated 永远是 false，导致已登录用户闪现表单（同 reset_password.rs 做法）。
    let initialized = *auth.initialized.read();
    let authenticated = auth.is_authenticated();
    let auth_for_auth_guard = auth.clone();
    use_effect(move || {
        if *auth_for_auth_guard.initialized.read() && auth_for_auth_guard.is_authenticated() {
            nav.replace(Route::Dashboard {});
        }
    });

    if !initialized || authenticated {
        return rsx! {
            Fragment {}
        };
    }

    rsx! {
        div { class: "ws-forgot",
            div { class: "ws-forgot__orb ws-forgot__orb--blue" }
            div { class: "ws-forgot__orb ws-forgot__orb--indigo" }

            div { class: "ws-forgot__card",
                div { class: "ws-forgot__icon" }
                h1 { class: "ws-forgot__title", {t.forgot_pw_title} }
                p { class: "ws-forgot__subtitle", {t.forgot_pw_subtitle} }

                form {
                    class: "ws-forgot__form",
                    onsubmit: move |e| {
                        e.prevent_default();
                        if *submitting.read() {
                            return;
                        }
                        let value = email.read().trim().to_string();
                        if value.is_empty() {
                            error_msg.set(Some(t.forgot_pw_email_empty.to_string()));
                            return;
                        }
                        if !value.contains('@') {
                            error_msg.set(Some(t.forgot_pw_email_invalid.to_string()));
                            return;
                        }
                        let auth_async = auth.clone();
                        let bus_async = log_bus;
                        let nav_async = nav;
                        let email_value = value;
                        submitting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            let path = "/api/public/auth/forgot-password".to_string();
                            let res = auth_async.client.forgot_password(&email_value).await;
                            push_log_result(bus_async, HttpMethod::Post, &path, &res);
                            submitting.set(false);
                            match res {
                                Ok(_) => {
                                    // 导航到重置密码页，邮箱预填。
                                    nav_async
                                        .replace(Route::ResetPassword {
                                            email: Some(email_value),
                                        });
                                }
                                Err(err) => {
                                    let msg = humanize_error(
                                        &err,
                                        ErrorContext::PasswordReset,
                                        i18n.lang(),
                                    );
                                    error_msg.set(Some(msg));
                                }
                            }
                        });
                    },
                    TextInput {
                        label: t.forgot_pw_email_label.to_string(),
                        placeholder: Some("name@domain.com".to_string()),
                        value: email,
                        input_type: InputType::Email,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("email".to_string()),
                        autocomplete: Some("email".to_string()),
                    }
                    if let Some(err) = error_msg.read().as_ref() {
                        p { class: "ws-form-error", "{err}" }
                    }
                    Button {
                        button_type: ButtonType::Submit,
                        full_width: true,
                        disabled: *submitting.read(),
                        loading: *submitting.read(),
                        "{t.forgot_pw_submit} [POST /forgot-password]"
                    }
                }

                div { class: "ws-forgot__back-row",
                    a {
                        class: "ws-forgot__back",
                        href: "#",
                        onclick: move |e| {
                            e.prevent_default();
                            nav.push(Route::LoginLanding {});
                        },
                        {t.forgot_pw_back_to_login}
                    }
                }
            }
        }
    }
}
