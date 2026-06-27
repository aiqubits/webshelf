//! 个人设置视图 —— 任何已认证用户可修改自己的密码。
//!
//! 流程：填写当前密码 + 新密码 + 确认新密码 → 提交到 POST /api/users/me/password。

use dioxus::prelude::*;
use ui::{Button, ButtonType, I18nContext, InputType, TextInput, Translations};

use crate::Route;
use crate::api::{ErrorContext, handle_unauth, humanize_error};
use crate::auth::AuthState;
use crate::balance::format_balance;
use crate::components::{ConfirmDialog, HttpMethod, LogBus, LogKind, push_log_result};

#[component]
pub fn Settings() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();
    let i18n = use_context::<I18nContext>();
    let t = i18n.t();

    let mut current_password = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut logging_out_all = use_signal(|| false);
    let mut show_logout_confirm = use_signal(|| false);
    let mut form_error = use_signal(|| Option::<String>::None);
    let mut success_msg = use_signal(|| Option::<String>::None);

    // Auth guard: 未认证用户不可见密码修改表单
    // 注意：必须先等待 initialization 完成（restore_from_storage_async），
    // 否则 authenticated 永远是 false，导致已登录用户被误跳转（Issue B1）。
    let initialized = *auth.initialized.read();
    let authenticated_at_render = auth.is_authenticated();
    let auth_for_guard = auth.clone();
    use_effect(move || {
        if *auth_for_guard.initialized.read() && !auth_for_guard.is_authenticated() {
            nav.replace(Route::LoginLanding {});
        }
        // 未初始化时不跳转，等待 restore 完成
    });

    if !initialized || !authenticated_at_render {
        return rsx! {
            Fragment {}
        };
    }

    // 切换 / 进入页面时清空表单
    {
        let mut cp = current_password;
        let mut np = new_password;
        let mut cnp = confirm_password;
        let mut fe = form_error;
        let mut sm = success_msg;
        use_effect(move || {
            cp.set(String::new());
            np.set(String::new());
            cnp.set(String::new());
            fe.set(None);
            sm.set(None);
        });
    }

    // Pre-clone for the onclick closure (logout-all) since `auth` and `nav`
    // are moved into the onsubmit closure (change-password).
    let auth_for_logout = auth.clone();
    let nav_for_logout = nav;
    let bus_for_logout = log_bus;

    let update_password_btn = format!(
        "{} [POST /api/users/me/password]",
        t.settings_update_password_btn
    );
    let logout_all_btn = format!(
        "{} [POST /api/users/me/logout-all]",
        t.settings_logout_all_btn
    );

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/settings.css") }
        div { class: "ws-settings",
            header { class: "ws-settings__header",
                div { class: "ws-settings__title-block",
                    h1 { class: "ws-settings__title", "{t.settings_title}" }
                    p { class: "ws-settings__subtitle", "{t.settings_subtitle}" }
                }
            }

            section { class: "ws-settings__section",
                h2 { class: "ws-settings__section-title", "{t.settings_account_title}" }
                div { class: "ws-settings__identity", {render_identity(auth.clone(), t)} }
            }

            section { class: "ws-settings__section",
                h2 { class: "ws-settings__section-title", "{t.settings_change_password_title}" }
                form {
                    class: "ws-settings__form",
                    onsubmit: move |e| {
                        e.prevent_default();
                        if *submitting.read() {
                            return;
                        }
                        // 同步校验：避免空字段浪费网络请求
                        if current_password.read().is_empty() {
                            form_error.set(Some(t.settings_validation_current_empty.to_string()));
                            return;
                        }
                        if new_password.read().is_empty() {
                            form_error.set(Some(t.settings_validation_new_empty.to_string()));
                            return;
                        }
                        if new_password.read().len() < 8 {
                            form_error.set(Some(t.settings_validation_new_short.to_string()));
                            return;
                        }
                        if new_password.read().cloned() != confirm_password.read().cloned() {
                            form_error.set(Some(t.settings_validation_new_mismatch.to_string()));
                            return;
                        }
                        if new_password.read().cloned() == current_password.read().cloned() {
                            form_error.set(Some(t.settings_validation_new_same_as_current.to_string()));
                            return;
                        }

                        let cp = current_password.read().clone();
                        let np = new_password.read().clone();
                        let client = auth.client.clone();
                        let bus = log_bus;
                        let mut auth_async = auth.clone();
                        let nav_async = nav;
                        let path = "/api/users/me/password".to_string();

                        submitting.set(true);
                        form_error.set(None);
                        success_msg.set(None);

                        spawn(async move {
                            let res = client.change_password(&cp, &np).await;
                            // 与 Dashboard / Users 保持一致：401/403 统一走 handle_unauth
                            // （注销 + 跳 /auth），避免在修改密码页会话失效时
                            // 只看到静态错误文案（Issue #3）。
                            // 传入 clone：handle_unauth 会 move auth_async，
                            // 后面 swap_token 还要再用一次。
                            if let Err(err) = &res
                                && handle_unauth(err, auth_async.clone(), nav_async, bus).await
                            {
                                submitting.set(false);
                                return;
                            }
                            push_log_result(bus, HttpMethod::Post, &path, &res);
                            submitting.set(false);
                            match res {
                                Ok(resp) => {
                                    // 服务端在事务内 `token_version += 1`，旧 JWT 永久失效；
                                    // 必须用 `new_token` 替换本地缓存，否则下一次 API 调用
                                    // 会 401 → 401 拦截器把用户踢回 /auth，"改密成功"实际上
                                    // 把用户踢下线（B1）。
                                    auth_async.swap_token(resp.new_token);
                                    success_msg.set(Some(resp.message));
                                    current_password.set(String::new());
                                    new_password.set(String::new());
                                    confirm_password.set(String::new());
                                }
                                Err(err) => {
                                    form_error
                                        .set(Some(humanize_error(&err, ErrorContext::UserManagement)));
                                }
                            }
                        });
                    },
                    TextInput {
                        label: t.settings_current_password_label.to_string(),
                        placeholder: Some(t.settings_current_password_placeholder.to_string()),
                        value: current_password,
                        input_type: InputType::Password,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("current_password".to_string()),
                        autocomplete: Some("current-password".to_string()),
                    }
                    TextInput {
                        label: t.settings_new_password_label.to_string(),
                        placeholder: Some(t.settings_new_password_placeholder.to_string()),
                        value: new_password,
                        input_type: InputType::Password,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("new_password".to_string()),
                        autocomplete: Some("new-password".to_string()),
                        hint: Some(t.settings_new_password_hint.to_string()),
                    }
                    TextInput {
                        label: t.settings_confirm_password_label.to_string(),
                        placeholder: Some(t.settings_confirm_password_placeholder.to_string()),
                        value: confirm_password,
                        input_type: InputType::Password,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("confirm_password".to_string()),
                        autocomplete: Some("new-password".to_string()),
                    }
                    if let Some(err) = form_error.read().as_ref() {
                        p { class: "ws-form-error", "{err}" }
                    }
                    if let Some(msg) = success_msg.read().as_ref() {
                        p { class: "ws-form-success", "{msg}" }
                    }
                    Button {
                        button_type: ButtonType::Submit,
                        full_width: true,
                        disabled: *submitting.read(),
                        loading: *submitting.read(),
                        "{update_password_btn}"
                    }
                }
            }

            section { class: "ws-settings__section",
                h2 { class: "ws-settings__section-title", "{t.settings_session_title}" }
                p { class: "ws-settings__desc",
                    "{t.settings_session_desc}"
                }
                Button {
                    button_type: ButtonType::Danger,
                    full_width: true,
                    disabled: *logging_out_all.read(),
                    loading: *logging_out_all.read(),
                    onclick: move |_| {
                        if *logging_out_all.read() {
                            return;
                        }
                        show_logout_confirm.set(true);
                    },
                    "{logout_all_btn}"
                }

                ConfirmDialog {
                    open: *show_logout_confirm.read(),
                    title: t.settings_confirm_logout_title.to_string(),
                    message: t.settings_confirm_logout_msg.to_string(),
                    danger: true,
                    loading: *logging_out_all.read(),
                    on_confirm: move |_| {
                        let mut auth_async = auth_for_logout.clone();
                        let nav_async = nav_for_logout;
                        let mut bus = bus_for_logout;
                        logging_out_all.set(true);
                        show_logout_confirm.set(false);
                        spawn(async move {
                            auth_async.logout_all_async().await;
                            bus.push(
                                HttpMethod::Post,
                                "/api/users/me/logout-all".to_string(),
                                "200".to_string(),
                                LogKind::Important,
                            );
                            nav_async.replace(Route::LoginLanding {});
                        });
                    },
                    on_cancel: move |_| show_logout_confirm.set(false),
                }
            }
        }
    }
}

fn render_identity(auth: AuthState, t: &Translations) -> Element {
    let snapshot = auth.user.read().clone();
    match snapshot {
        Some(user) => rsx! {
            div { class: "ws-settings__identity-row",
                span { class: "ws-settings__identity-label", "{t.settings_account_label}" }
                span { class: "ws-settings__identity-value", "{user.name}" }
            }
            div { class: "ws-settings__identity-row",
                span { class: "ws-settings__identity-label", "{t.settings_email_label}" }
                span { class: "ws-settings__identity-value", "{user.email}" }
            }
            div { class: "ws-settings__identity-row",
                span { class: "ws-settings__identity-label", "{t.settings_role_label}" }
                span { class: "ws-settings__identity-value", "{user.role}" }
            }
            div { class: "ws-settings__identity-row",
                span { class: "ws-settings__identity-label", "{t.settings_balance_label}" }
                span { class: "ws-settings__identity-value", "{format_balance(user.balance)}" }
            }
        },
        None => rsx! {
            span { class: "ws-settings__identity-value", "{t.settings_not_logged_in}" }
        },
    }
}
