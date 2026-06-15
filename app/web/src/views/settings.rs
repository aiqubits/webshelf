//! 个人设置视图 —— 任何已认证用户可修改自己的密码。
//!
//! 流程：填写当前密码 + 新密码 + 确认新密码 → 提交到 POST /api/users/me/password。

use dioxus::prelude::*;
use ui::{Button, ButtonType, InputType, TextInput};

use crate::Route;
use crate::api::{ErrorContext, handle_unauth, humanize_error};
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, push_log_result};

#[component]
pub fn Settings() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    let mut current_password = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
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
            nav.replace(Route::Auth {});
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

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/settings.css") }
        div { class: "ws-settings",
            header { class: "ws-settings__header",
                div { class: "ws-settings__title-block",
                    h1 { class: "ws-settings__title", "个人设置" }
                    p { class: "ws-settings__subtitle", "修改账户密码以保护账号安全" }
                }
            }

            section { class: "ws-settings__section",
                h2 { class: "ws-settings__section-title", "账户身份" }
                div { class: "ws-settings__identity", {render_identity(auth.clone())} }
            }

            section { class: "ws-settings__section",
                h2 { class: "ws-settings__section-title", "修改密码" }
                form {
                    class: "ws-settings__form",
                    onsubmit: move |e| {
                        e.prevent_default();
                        if *submitting.read() {
                            return;
                        }
                        // 同步校验：避免空字段浪费网络请求
                        if current_password.read().is_empty() {
                            form_error.set(Some("请输入当前密码".into()));
                            return;
                        }
                        if new_password.read().is_empty() {
                            form_error.set(Some("请输入新密码".into()));
                            return;
                        }
                        if new_password.read().len() < 8 {
                            form_error.set(Some("新密码至少需要 8 个字符".into()));
                            return;
                        }
                        if new_password.read().cloned() != confirm_password.read().cloned() {
                            form_error.set(Some("两次输入的新密码不一致".into()));
                            return;
                        }
                        if new_password.read().cloned() == current_password.read().cloned() {
                            form_error.set(Some("新密码不能与当前密码相同".into()));
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
                                && handle_unauth(err, auth_async.clone(), nav_async, bus)
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
                        label: "当前密码".to_string(),
                        placeholder: Some("请输入当前密码".to_string()),
                        value: current_password,
                        input_type: InputType::Password,
                        required: true,
                        disabled: *submitting.read(),
                        name: Some("current_password".to_string()),
                        autocomplete: Some("current-password".to_string()),
                    }
                    TextInput {
                        label: "新密码".to_string(),
                        placeholder: Some("≥8 字符，包含大小写字母、数字和 ASCII 标点".to_string()),
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
                        "更新密码 [POST /api/users/me/password]"
                    }
                }
            }
        }
    }
}

fn render_identity(auth: AuthState) -> Element {
    let snapshot = auth.user.read().clone();
    match snapshot {
        Some(user) => rsx! {
            div { class: "ws-settings__identity-row",
                span { class: "ws-settings__identity-label", "账户名" }
                span { class: "ws-settings__identity-value", "{user.name}" }
            }
            div { class: "ws-settings__identity-row",
                span { class: "ws-settings__identity-label", "邮箱地址" }
                span { class: "ws-settings__identity-value", "{user.email}" }
            }
            div { class: "ws-settings__identity-row",
                span { class: "ws-settings__identity-label", "角色" }
                span { class: "ws-settings__identity-value", "{user.role}" }
            }
        },
        None => rsx! {
            span { class: "ws-settings__identity-value", "未登录" }
        },
    }
}
