//! Auth 视图 —— 登录 / 注册。
//!
//! 接入 `AuthState::login` / `AuthState::register`，
//! 成功后写入 LogBus + 跳转 `/`。

use dioxus::prelude::*;
use ui::{AuthForm, AuthMode, AuthPayload};

use crate::Route;
use crate::api::{ErrorContext, humanize_error};
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, push_log_result};

#[component]
pub fn Auth() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    // 会话恢复已上移至 App 组件 (main.rs)，此处不再重复触发——
    // 避免“记住登录”后直接访问受保护路由 (e.g. /users) 表现未登录的 BUG。
    // 此处仅检查初始化状态以避免闪现登录表单。
    if !*auth.initialized.read() {
        return rsx! {
            Fragment {}
        };
    }

    // 已登录则直接跳到 dashboard
    let authenticated_at_render = auth.is_authenticated();
    let auth_for_effect = auth.clone();
    use_effect(move || {
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

    // 每次切换登录 / 注册模式时清空表单与状态，避免跨模式残留。
    // mode() 调用建立信号追踪，仅 mode 变化时触发 effect；
    // .set() 不产生订阅，因此不会意外清空用户输入。
    use_effect(move || {
        let _ = mode();
        name.set(String::new());
        email.set(String::new());
        password.set(String::new());
        password_confirm.set(String::new());
        error_msg.set(None);
        // 中断前一个模式可能尚在执行中的异步请求，
        // 避免用户在 Register 页面看到 Login 的错误信息。
        loading.set(false);
    });

    rsx! {
        div { class: "ws-auth-view",
            AuthForm {
                mode,
                name,
                email,
                password,
                password_confirm,
                remember: Some(remember),
                loading: *loading.read(),
                error: error_msg.read().clone(),
                on_submit: move |payload: AuthPayload| {
                    if *loading.read() {
                        return;
                    }
                    // 前端表单校验 —— 避免空字段浪费网络请求和服务器资源。
                    if payload.email.trim().is_empty() {
                        error_msg.set(Some("邮箱地址不能为空".into()));
                        return;
                    }
                    if payload.password.is_empty() {
                        error_msg.set(Some("密码不能为空".into()));
                        return;
                    }
                    if payload.mode == AuthMode::Register {
                        if payload.name.trim().is_empty() {
                            error_msg.set(Some("用户名不能为空".into()));
                            return;
                        }
                        if payload.password != payload.password_confirm {
                            error_msg.set(Some("两次输入的密码不一致".into()));
                            return;
                        }
                    }
                    let payload_email = payload.email.clone();
                    let payload_password = payload.password.clone();
                    let payload_name = payload.name.clone();
                    let payload_mode = payload.mode;
                    let payload_remember = payload.remember;

                    // 复制 Context 句柄供 async 块使用（EventHandler 闭包必须是 FnMut）。
                    let mut auth_async = auth.clone();
                    let bus_async = log_bus;

                    loading.set(true);
                    error_msg.set(None);

                    let mode_check = mode;

                    spawn(async move {
                        let result = match payload_mode {
                            AuthMode::Login => {
                                let path = "/api/public/auth/login".to_string();
                                let res = auth_async
                                    .login(&payload_email, &payload_password, payload_remember)
                                    .await;
                                // 仅当表单模式未切换时才记录日志，避免过期请求污染 Toast/Console
                                if *mode_check.read() == AuthMode::Login {
                                    push_log_result(bus_async, HttpMethod::Post, &path, &res);
                                }
                                res.map(|_| "Login".to_string())
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
                                res.map(|_| "Register".to_string())
                            }
                        };

                        loading.set(false);

                        match result {
                            Ok(_) => {} // 导航由 auth.user 变化触发的 use_effect 统一处理，避免双重导航
                            Err(err) => {
                                // 仅当表单模式未切换时才展示错误信息，
                                // 避免 Login 的报错污染已切换到 Register 的页面。
                                if *mode_check.read() == payload_mode {
                                    error_msg.set(Some(humanize_error(&err, ErrorContext::Auth)));
                                }
                            }
                        }
                    });
                },
            }
        }
    }
}
