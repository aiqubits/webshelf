//! 重置密码视图 —— `/reset-password` (或 `/reset-password/:email`)。
//!
//! 流程：用户输入邮箱 + 6 位验证码 + 新密码 → 提交到
//! `POST /api/public/auth/reset-password`。成功时服务端原子地
//! `token_version += 1` 并返回全新 JWT，前端按登录流程接管会话。
//!
//! 失败统一展示"验证码无效或已过期"或"密码强度不足"（服务端 anti-enumeration）。

use dioxus::prelude::*;

use ui::{Button, ButtonType, InputType, TextInput};

use crate::Route;
use crate::api::{ErrorContext, humanize_error};
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, push_log_result};

#[component]
pub fn ResetPassword(email: Option<String>) -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    let email_signal = use_signal(|| email.clone().unwrap_or_default());
    let code = use_signal(String::new);
    let new_password = use_signal(String::new);
    let confirm_password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);

    // 已登录守卫：已登录用户改密应走 /settings（修改密码），
    // 而非通过忘记密码的重置流程。
    // 在首次渲染时即检查并返回空 Fragment，避免渲染完整表单后闪跳。
    let authenticated = auth.is_authenticated();
    let auth_for_auth_guard = auth.clone();
    use_effect(move || {
        if auth_for_auth_guard.is_authenticated() {
            nav.replace(Route::Settings {});
        }
    });

    if authenticated {
        return rsx! {
            Fragment {}
        };
    }

    rsx! {
        div { class: "ws-reset",
            div { class: "ws-reset__orb ws-reset__orb--cyan" }
            div { class: "ws-reset__orb ws-reset__orb--purple" }

            div { class: "ws-reset__card",
                div { class: "ws-reset__icon" }
                h1 { class: "ws-reset__title", "重置密码" }
                p { class: "ws-reset__subtitle",
                    "输入注册邮箱和邮件中的 6 位验证码，设置新密码"
                }

                form {
                    class: "ws-reset__form",
                    onsubmit: move |e| {
                        e.prevent_default();
                        if *submitting.read() {
                            return;
                        }
                        let email_value = email_signal.read().trim().to_string();
                        let code_value = code.read().trim().to_string();
                        let new_pw = new_password.read().clone();
                        let confirm_pw = confirm_password.read().clone();

                        // 前置同步校验 —— 避免空字段浪费网络请求。
                        if email_value.is_empty() {
                            error_msg.set(Some("请输入注册邮箱".into()));
                            return;
                        }
                        if !email_value.contains('@') {
                            error_msg.set(Some("邮箱格式不正确".into()));
                            return;
                        }
                        if code_value.len() != 6 || !code_value.chars().all(|c| c.is_ascii_digit()) {
                            error_msg.set(Some("验证码格式错误（应为 6 位数字）".into()));
                            return;
                        }
                        if new_pw.len() < 8 {
                            error_msg.set(Some("新密码至少需要 8 个字符".into()));
                            return;
                        }
                        if new_pw != confirm_pw {
                            error_msg.set(Some("两次输入的密码不一致".into()));
                            return;
                        }
                        let mut auth_async = auth.clone();
                        let bus_async = log_bus;
                        // 沿用旧会话的持久化偏好：若 localStorage 中有旧 token，
                        // 说明用户此前选择了「记住我」，重置后的新会话也应持久化，
                        // 避免用户刷新页面后被意外登出。
                        let was_remembered = crate::auth::load_token().is_some();
                        submitting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            let path = "/api/public/auth/reset-password".to_string();
                            let res = auth_async
                                .client
                                .reset_password(&email_value, &code_value, &new_pw)
                                .await;
                            push_log_result(bus_async, HttpMethod::Post, &path, &res);
                            submitting.set(false);
                            match res {
                                Ok(resp) => {
                                    let _ = resp.message;
                                    auth_async.persist_session_async(&resp.token, was_remembered).await;
                                    nav.replace(Route::Dashboard {});
                                }
                                Err(err) => {
                                    let msg = humanize_error(&err, ErrorContext::PasswordReset);
                                    error_msg.set(Some(msg));
                                }
                            }
                        });
                    },
                    TextInput {
                        label: "注册邮箱".to_string(),
                        placeholder: Some("name@domain.com".to_string()),
                        value: email_signal,
                        input_type: InputType::Email,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("email".to_string()),
                        autocomplete: Some("email".to_string()),
                    }
                    TextInput {
                        label: "验证码 (邮件中的 6 位数字)".to_string(),
                        placeholder: Some("000000".to_string()),
                        value: code,
                        input_type: InputType::Number,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("code".to_string()),
                        autocomplete: Some("one-time-code".to_string()),
                        hint: Some("从密码重置邮件中获取，10 分钟内有效".to_string()),
                    }
                    TextInput {
                        label: "新密码".to_string(),
                        placeholder: Some("≥8 字符，含大小写字母、数字和 ASCII 标点".to_string()),
                        value: new_password,
                        input_type: InputType::Password,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("new_password".to_string()),
                        autocomplete: Some("new-password".to_string()),
                        hint: Some(
                            "密码需 ≥8 字符，包含大小写字母、数字和 ASCII 标点"
                                .to_string(),
                        ),
                    }
                    TextInput {
                        label: "确认新密码".to_string(),
                        placeholder: Some("再次输入新密码".to_string()),
                        value: confirm_password,
                        input_type: InputType::Password,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("confirm_password".to_string()),
                        autocomplete: Some("new-password".to_string()),
                    }
                    if let Some(err) = error_msg.read().as_ref() {
                        p { class: "ws-form-error", "{err}" }
                    }
                    Button {
                        button_type: ButtonType::Submit,
                        full_width: true,
                        disabled: *submitting.read(),
                        loading: *submitting.read(),
                        "重置密码并登录 [POST /reset-password]"
                    }
                }

                div { class: "ws-reset__back-row",
                    a {
                        class: "ws-reset__back",
                        href: "#",
                        onclick: move |e| {
                            e.prevent_default();
                            nav.push(Route::Auth {});
                        },
                        "← 返回登录"
                    }
                }
            }
        }
    }
}
