//! LoginLanding 视图 —— `/` 根路由。
//!
//! 左侧为登录/注册表单（复用 AuthForm），右侧展示公众号二维码、版权声明与 GitHub 项目地址。
//! 已登录用户自动跳转到 `/dashboard`。

use dioxus::prelude::*;
use ui::{AuthForm, AuthMode, AuthPayload, I18nContext, LanguageSwitcher, LanguageSwitcherVariant};

use crate::Route;
use crate::api::{ErrorContext, humanize_error};
use crate::auth::{AuthState, RegisterOutcome};
use crate::components::{HttpMethod, LogBus, push_log_result};

const QRCODE_IMG: Asset = asset!("/assets/qrcode-op.jpg");

/// 提交表单后的导航动作。
enum SubmitAction {
    Nothing,
    NavigateToVerify { email: String },
}

#[component]
pub fn LoginLanding() -> Element {
    let i18n = use_context::<I18nContext>();
    let t = i18n.t();
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    // 等待 AuthState 初始化完成，避免「记住登录」用户首屏被误判为未登录
    // 在 restore_from_storage_async 进行中（initialized=false），user 为 None，
    // 如果不检查 initialized 会导致已登录用户短暂看到登录表单
    let is_initialized = *auth.initialized.read();
    if !is_initialized {
        return rsx! {
            Fragment {}
        };
    }

    // 进入登录页时清空 pending 残留
    let mut auth_for_clear = auth.clone();
    use_effect(move || {
        auth_for_clear.clear_pending_registration();
    });

    // 已登录则直接跳到 dashboard
    let authenticated_at_render = auth.is_authenticated();
    let auth_for_effect = auth.clone();
    use_effect(move || {
        // 显式读取 auth.user 以建立响应式依赖
        // 当登录成功后 auth.user 变化时，此 effect 会重新执行并触发导航
        if auth_for_effect.is_authenticated() {
            nav.replace(Route::Dashboard {});
        }
    });

    // 渲染时前置判断：已登录时不渲染表单，消除首帧闪现
    if authenticated_at_render {
        return rsx! {
            Fragment {}
        };
    }

    let mode = use_signal(AuthMode::default);
    let mut name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut password_confirm = use_signal(String::new);
    let remember = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);

    // 每次切换登录/注册模式时清空表单与状态
    use_effect(move || {
        let _ = mode();
        name.set(String::new());
        email.set(String::new());
        password.set(String::new());
        password_confirm.set(String::new());
        error_msg.set(None);
        loading.set(false);
    });

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/login_landing.css"),
        }
        div { class: "ws-landing",
            // ── 左侧：登录/注册表单 ──
            div { class: "ws-landing__left",
                div { class: "ws-landing__brand",
                    h1 { class: "ws-landing__brand-title", "WebShelf" }
                    p { class: "ws-landing__brand-subtitle", {t.login_brand_subtitle} }
                }
                AuthForm {
                    mode,
                    name,
                    email,
                    password,
                    password_confirm,
                    remember: Some(remember),
                    loading: *loading.read(),
                    error: error_msg.read().clone(),
                    on_forgot: move |_: MouseEvent| {
                        nav.push(Route::ForgotPassword {});
                    },
                    on_submit: move |payload: AuthPayload| {
                        if *loading.read() {
                            return;
                        }
                        // 前端表单校验
                        if payload.email.trim().is_empty() {
                            error_msg.set(Some(t.login_email_empty.to_string()));
                            return;
                        }
                        if payload.password.is_empty() {
                            error_msg.set(Some(t.login_password_empty.to_string()));
                            return;
                        }
                        if payload.mode == AuthMode::Register {
                            if payload.name.trim().is_empty() {
                                error_msg.set(Some(t.login_name_empty.to_string()));
                                return;
                            }
                            if payload.password != payload.password_confirm {
                                error_msg.set(Some(t.login_password_mismatch.to_string()));
                                return;
                            }
                        }
                        let payload_email = payload.email.clone();
                        let payload_password = payload.password.clone();
                        let payload_name = payload.name.clone();
                        let payload_mode = payload.mode;
                        let payload_remember = payload.remember;

                        let mut auth_async = auth.clone();
                        let bus_async = log_bus;
                        let nav_async = nav;

                        loading.set(true);
                        error_msg.set(None);

                        let mode_check = mode;

                        spawn(async move {
                            let result: Result<SubmitAction, client_api::ClientError> = match payload_mode {
                                AuthMode::Login => {
                                    let path = "/api/public/auth/login".to_string();
                                    let res = auth_async
                                        .login(&payload_email, &payload_password, payload_remember)
                                        .await;
                                    if *mode_check.read() == AuthMode::Login {
                                        push_log_result(bus_async, HttpMethod::Post, &path, &res);
                                    }
                                    res.map(|_| SubmitAction::Nothing)
                                }
                                AuthMode::Register => {
                                    let path = "/api/public/auth/register".to_string();
                                    let res = auth_async
                                        .register(
                                            &payload_email,
                                            &payload_password,
                                            &payload_name,
                                            payload_remember,
                                        )
                                        .await;
                                    if *mode_check.read() == AuthMode::Register {
                                        push_log_result(bus_async, HttpMethod::Post, &path, &res);
                                    }
                                    res.map(|outcome| match outcome {
                                        RegisterOutcome::LoggedIn => SubmitAction::Nothing,
                                        RegisterOutcome::NeedsVerification { email } => {
                                            SubmitAction::NavigateToVerify {
                                                email,
                                            }
                                        }
                                    })
                                }
                            };

                            loading.set(false);

                            match result {
                                Ok(SubmitAction::Nothing) => {} // 导航由 auth.user 变化触发的 use_effect 统一处理
                                Ok(SubmitAction::NavigateToVerify { email }) => {
                                    nav_async.push(Route::VerifyEmail { email });
                                }
                                Err(err) => {
                                    if *mode_check.read() == payload_mode {
                                        error_msg
                                            .set(
                                                Some(humanize_error(&err, ErrorContext::Auth, i18n.lang())),
                                            );
                                    }
                                }
                            }
                        });
                    },
                }
            }

            // ── 右侧：二维码 + 版权 + GitHub ──
            div { class: "ws-landing__right",
                div { class: "ws-landing__info-card",
                    // 公众号二维码区域
                    div { class: "ws-landing__qr-section",
                        div { class: "ws-landing__qr-placeholder",
                            img {
                                class: "ws-landing__qr-img",
                                src: QRCODE_IMG,
                                alt: "openpick qrcode",
                            }
                            p { class: "ws-landing__qr-label", {t.login_qr_label} }
                            p { class: "ws-landing__qr-hint", {t.login_qr_hint} }
                        }
                    }

                    // 分隔线
                    div { class: "ws-landing__divider" }

                    // GitHub 项目地址
                    div { class: "ws-landing__github",
                        a {
                            class: "ws-landing__github-link",
                            href: "https://github.com/aiqubits/webshelf",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            svg {
                                width: "20",
                                height: "20",
                                view_box: "0 0 24 24",
                                fill: "currentColor",
                                path { d: "M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" }
                            }
                            span { {t.login_github_label} }
                        }
                    }

                    // 版权声明
                    div { class: "ws-landing__copyright",
                        p { class: "ws-landing__copyright-text",
                            "© 2026 WebShelf. All rights reserved."
                        }
                        p { class: "ws-landing__copyright-sub", {t.login_copyright_sub} }
                    }
                }
            }
        }

        // 语言切换浮动按钮
        LanguageSwitcher { variant: LanguageSwitcherVariant::Floating }
    }
}
