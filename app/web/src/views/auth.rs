//! Auth 视图 —— 登录 / 注册。
//!
//! 接入 `AuthState::login` / `AuthState::register`，
//! 成功后写入 LogBus + 跳转 `/`。

use client_api::ClientError;
use dioxus::prelude::*;
use ui::{AuthForm, AuthMode, AuthPayload};

use crate::Route;
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, LogKind};

#[component]
pub fn Auth() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    // 已登录则直接跳到 dashboard
    let auth_for_effect = auth.clone();
    use_effect(move || {
        if auth_for_effect.is_authenticated() {
            nav.push(Route::Dashboard {});
        }
    });

    let mode = use_signal(AuthMode::default);
    let name = use_signal(String::new);
    let email = use_signal(String::new);
    let password = use_signal(String::new);
    let remember = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "ws-auth-view",
            AuthForm {
                mode,
                name,
                email,
                password,
                remember,
                loading: *loading.read(),
                error: error_msg.read().clone(),
                on_submit: move |payload: AuthPayload| {
                    if *loading.read() {
                        return;
                    }
                    let payload_email = payload.email.clone();
                    let payload_password = payload.password.clone();
                    let payload_name = payload.name.clone();
                    let payload_mode = payload.mode;
                    let payload_remember = payload.remember;

                    // 复制 Context 句柄供 async 块使用（EventHandler 闭包必须是 FnMut）。
                    let mut auth_async = auth.clone();
                    let bus_async = log_bus;
                    let nav_async = nav;

                    loading.set(true);
                    error_msg.set(None);

                    spawn(async move {
                        let result = match payload_mode {
                            AuthMode::Login => {
                                let path = "/api/public/auth/login".to_string();
                                let res = auth_async
                                    .login(&payload_email, &payload_password, payload_remember)
                                    .await;
                                push_log(&bus_async, HttpMethod::Post, &path, &res);
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
                                push_log(&bus_async, HttpMethod::Post, &path, &res);
                                res.map(|_| "Register".to_string())
                            }
                        };

                        loading.set(false);

                        match result {
                            Ok(_) => {
                                nav_async.replace(Route::Dashboard {});
                            }
                            Err(err) => {
                                error_msg.set(Some(humanize_error(&err)));
                            }
                        }
                    });
                },
            }
        }
    }
}

/// 把 client-api 的错误翻译成中文提示。
fn humanize_error(err: &ClientError) -> String {
    match err {
        ClientError::Network(msg) => format!("网络异常: {msg}"),
        ClientError::ServerError(status, body) => {
            format!("服务器错误 (HTTP {status}): {body}")
        }
        ClientError::Other(status, body) => {
            let code = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| v.get("error").and_then(|c| c.as_str().map(String::from)))
                .unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str().map(String::from)))
                .unwrap_or_else(|| body.clone());
            match (status, code.as_str()) {
                (401, _) => "邮箱或密码错误".to_string(),
                (_, "validation_error") => format!("参数错误: {msg}"),
                (_, "conflict") => "该邮箱已注册".to_string(),
                _ => format!("请求失败 (HTTP {status}): {msg}"),
            }
        }
        ClientError::RateLimited(_) => "请求过于频繁，请稍后再试".to_string(),
        ClientError::Deserialization(msg) => format!("响应解析失败: {msg}"),
        ClientError::Config(msg) => format!("客户端配置错误: {msg}"),
        _ => format!("未知错误: {err}"),
    }
}

fn push_log<T>(bus: &LogBus, method: HttpMethod, path: &str, res: &Result<T, ClientError>) {
    let mut bus = *bus;
    match res {
        Ok(_) => bus.push(
            method,
            path.to_string(),
            "200 OK".to_string(),
            LogKind::Success,
        ),
        Err(err) => {
            let status = match err {
                ClientError::Other(s, _) | ClientError::ServerError(s, _) => s.to_string(),
                _ => "ERR".to_string(),
            };
            bus.push(method, path.to_string(), status, LogKind::Error);
        }
    }
}
