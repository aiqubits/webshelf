//! 邮件验证码视图 —— `/verify-email/:email`。
//!
//! 流程：
//! 1. mount 时启动 60s 重发倒计时；若 `pending_registration` 不匹配或缺失，
//!    视为脏状态，重定向 `/auth`。
//! 2. 用户输入 6 位验证码 → `POST /verify-email`。
//! 3. 失败计数 +1；累计 5 次后禁用验证按钮并提示重发。
//! 4. 成功 → 自动 `POST /login`（使用 pending 中的密码） → `auth.user` 变化
//!    触发 App 层 use_effect 跳 `/`。
//! 5. "重新发送"按钮：倒计时为 0 时启用，触发 `POST /resend-code` 后重置
//!    倒计时 60s 并清零失败计数。

use dioxus::prelude::*;

use crate::Route;
use crate::api::{ErrorContext, humanize_error};
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, push_log_result};

/// 服务端 `MAX_FAILED_ATTEMPTS = 5`：与 `server/src/services/verification.rs:16` 保持一致。
/// 客户端提前禁用是纯 UX 优化（服务端在第 5 次也会拒绝）。
const MAX_CLIENT_ATTEMPTS: i32 = 5;

/// 服务端 `RESEND_COOLDOWN_SECONDS = 60`：与 `server/src/services/verification.rs:15` 保持一致。
const RESEND_COOLDOWN_SECS: i32 = 60;

#[component]
pub fn VerifyEmail(email: String) -> Element {
    // ── 所有 hook 必须在组件顶部无条件调用 ─────────────────────────
    // Dioxus hook 按调用顺序索引；early return 前后的 hook 数量不一致
    // 会导致后续 render 的 hook 状态错位甚至 panic。
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    let mut code = use_signal(String::new);
    let mut countdown = use_signal(|| RESEND_COOLDOWN_SECS);
    let mut failed_attempts = use_signal(|| 0_i32);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut info_msg = use_signal(|| Option::<String>::None);

    // ── 路由守卫（副作用：render 后异步执行） ───────────────────────
    // 已登录用户访问此路由直接跳走（防止登录后被卡在验证页）。
    let auth_for_auth_guard = auth.clone();
    use_effect(move || {
        if auth_for_auth_guard.is_authenticated() {
            nav.replace(Route::Dashboard {});
        }
    });

    // ── 脏状态守卫（副作用：render 后异步执行） ─────────────────────
    // 注册临时态缺失或邮箱不匹配（典型场景：刷新 /verify-email 后 pending 被 GC，
    // 或用户直接深链到他人邮箱的验证页）。统一跳回 /auth。
    //
    // 必须先检查 `is_authenticated()`：
    // verify-email 成功后 `take_pending_registration` 会把 pending 清空，
    // 同一 tick 内 auth.user 也变为 Some()。auth_guard 触发
    // nav.replace(Dashboard)，dirty_guard 不检查登录态就会紧随其后
    // 触发 nav.replace(Auth) 并覆盖，导致刚验证完的用户卡在登录页。
    let auth_for_dirty_guard = auth.clone();
    let email_for_dirty_guard = email.clone();
    use_effect(move || {
        if auth_for_dirty_guard.is_authenticated() {
            return;
        }
        let matches = auth_for_dirty_guard
            .pending_registration
            .read()
            .as_ref()
            .map(|p| p.email.eq_ignore_ascii_case(&email_for_dirty_guard))
            .unwrap_or(false);
        if !matches {
            let mut auth_clone = auth_for_dirty_guard.clone();
            auth_clone.clear_pending_registration();
            nav.replace(Route::Auth {});
        }
    });

    // ── 倒计时循环 ────────────────────────────────────────────────
    // 用 `use_coroutine` 而不是 `use_effect` + `spawn`：
    // 前者在组件 unmount 时自动取消任务、释放 signal 引用；
    // 后者会让 spawned task 永远运行、泄漏 countdown signal 的 Arc。
    // 循环常驻不退出：countdown 归零时不能 break，否则 resend 把 countdown
    // 重置为 60 后原 task 已终止，倒计时会卡在 60s 不动。
    use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            client_api::Client::sleep_ms(1000).await;
            let v = *countdown.peek();
            if v > 0 {
                countdown.set(v - 1);
            }
        }
    });

    // ── 同步守卫判断（所有 hook 已调用完毕，可安全 early return） ──
    let authenticated_at_render = auth.is_authenticated();
    let pending_email_matches = auth
        .pending_registration
        .read()
        .as_ref()
        .map(|p| p.email.eq_ignore_ascii_case(&email))
        .unwrap_or(false);

    if authenticated_at_render || !pending_email_matches {
        return rsx! { Fragment {} };
    }

    // ── 提交验证码 ────────────────────────────────────────────────
    // 用一个共享的闭包 `do_verify` 让按钮点击和表单 submit（Enter 键）
    // 共享同一份逻辑。闭包必须可多次调用 (FnMut)。
    let auth_for_verify = auth.clone();
    let email_for_verify = email.clone();
    let mut do_verify = move || {
        if *loading.read() {
            return;
        }
        let code_value = code.read().trim().to_string();
        if code_value.len() != 6 || !code_value.chars().all(|c| c.is_ascii_digit()) {
            error_msg.set(Some("请输入 6 位数字验证码".to_string()));
            return;
        }
        if *failed_attempts.read() >= MAX_CLIENT_ATTEMPTS {
            error_msg.set(Some(
                "尝试次数过多，请点击下方按钮重新发送验证码".to_string(),
            ));
            return;
        }

        let email_inner = email_for_verify.clone();
        let code_inner = code_value;
        let mut auth_async = auth_for_verify.clone();
        let bus_async = log_bus;

        loading.set(true);
        error_msg.set(None);
        info_msg.set(None);

        spawn(async move {
            let path = "/api/public/auth/verify-email".to_string();
            let res = auth_async
                .client
                .verify_email(&email_inner, &code_inner)
                .await;
            push_log_result(bus_async, HttpMethod::Post, &path, &res);

            match res {
                Ok(_) => {
                    // 验证成功 → 取出 pending → 自动 login
                    if let Some(pending) = auth_async.take_pending_registration() {
                        let path_login = "/api/public/auth/login".to_string();
                        let login_res = auth_async
                            .login(&pending.email, &pending.password, pending.remember)
                            .await;
                        push_log_result(bus_async, HttpMethod::Post, &path_login, &login_res);
                        if let Err(err) = login_res {
                            // 自动登录失败（理论上不会发生，除非密码在用户不知情时被改）。
                            // 跳回 /auth 让用户重新登录。
                            let msg = humanize_error(&err, ErrorContext::EmailVerification);
                            error_msg.set(Some(format!(
                                "验证成功，但自动登录失败：{msg}，请返回登录页"
                            )));
                            loading.set(false);
                            return;
                        }
                        // 成功：use_effect 监听 auth.user 变化会跳到 /
                        loading.set(false);
                    } else {
                        // 极端情况：pending 在 verify 过程中被外部清空
                        error_msg.set(Some("会话已过期，请重新登录".to_string()));
                        loading.set(false);
                    }
                }
                Err(err) => {
                    loading.set(false);
                    let attempts = *failed_attempts.read() + 1;
                    failed_attempts.set(attempts);
                    let mut msg = humanize_error(&err, ErrorContext::EmailVerification);
                    if attempts >= MAX_CLIENT_ATTEMPTS {
                        msg.push_str("（已达最大尝试次数）");
                    }
                    error_msg.set(Some(msg));
                }
            }
        });
    };

    // ── 重新发送 ──────────────────────────────────────────────────
    let auth_for_resend = auth.clone();
    let email_for_resend = email.clone();
    let on_resend = move |_evt: MouseEvent| {
        if *loading.read() || *countdown.read() > 0 {
            return;
        }
        let email_inner = email_for_resend.clone();
        let auth_async = auth_for_resend.clone();
        let bus_async = log_bus;

        loading.set(true);
        error_msg.set(None);
        info_msg.set(None);

        spawn(async move {
            let path = "/api/public/auth/resend-code".to_string();
            let res = auth_async.client.resend_code(&email_inner).await;
            push_log_result(bus_async, HttpMethod::Post, &path, &res);

            loading.set(false);
            match res {
                Ok(_) => {
                    // 重置倒计时与失败计数
                    countdown.set(RESEND_COOLDOWN_SECS);
                    failed_attempts.set(0);
                    code.set(String::new());
                    info_msg.set(Some("新验证码已发送至您的邮箱".to_string()));
                }
                Err(err) => {
                    let msg = humanize_error(&err, ErrorContext::EmailVerification);
                    // 若服务端提示冷却未到，强制倒计时为 10s 避免用户立刻再点
                    if matches!(err, client_api::ClientError::Other(400, _)) {
                        countdown.set(10);
                    }
                    error_msg.set(Some(msg));
                }
            }
        });
    };

    // ── 返回登录页 ────────────────────────────────────────────────
    // 不接事件参数：在 <a> 的 onclick 闭包内部直接调用。
    // 闭包体调用了 &mut self 方法，需声明 mut 才能调用（FnMut）。
    let mut auth_for_back = auth.clone();
    let mut on_back = move || {
        auth_for_back.clear_pending_registration();
        nav.replace(Route::Auth {});
    };

    let resend_label = if *countdown.read() > 0 {
        format!("重新发送 ({}s)", *countdown.read())
    } else {
        "重新发送验证码".to_string()
    };
    let code_value = code.read().trim().to_string();
    let verify_disabled =
        *loading.read() || *failed_attempts.read() >= MAX_CLIENT_ATTEMPTS || code_value.len() != 6;
    let locked = *failed_attempts.read() >= MAX_CLIENT_ATTEMPTS;

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/verify_email.css"),
        }
        div { class: "ws-verify",
            div { class: "ws-verify__orb ws-verify__orb--blue" }
            div { class: "ws-verify__orb ws-verify__orb--cyan" }

            div { class: "ws-verify__card",
                div { class: "ws-verify__icon" }

                h1 { class: "ws-verify__title", "验证您的邮箱" }
                p { class: "ws-verify__subtitle",
                    "验证码已发送至 "
                    strong { "{email}" }
                    " ，10 分钟内有效"
                }

                form {
                    class: "ws-verify__form",
                    onsubmit: move |e| {
                        e.prevent_default();
                        do_verify();
                    },
                    label { class: "ws-verify__label", "6 位数字验证码" }
                    input {
                        class: "ws-verify__code-input",
                        r#type: "text",
                        inputmode: "numeric",
                        autocomplete: "one-time-code",
                        maxlength: "6",
                        pattern: "[0-9]{6}",
                        placeholder: "000000",
                        disabled: locked || *loading.read(),
                        value: "{code.read()}",
                        oninput: move |e| {
                            let v: String = e
                                .value()
                                .chars()
                                .filter(|c| c.is_ascii_digit())
                                .take(6)
                                .collect();
                            code.set(v);
                            error_msg.set(None);
                        },
                    }
                    p { class: "ws-verify__hint", "请检查垃圾邮件夹" }

                    if let Some(info) = info_msg.read().as_ref() {
                        p { class: "ws-verify__info", "{info}" }
                    }
                    if let Some(err) = error_msg.read().as_ref() {
                        p { class: "ws-verify__error", "{err}" }
                    }

                    if locked {
                        div { class: "ws-verify__locked",
                            strong { "已达最大尝试次数" }
                            " — 请重新发送验证码以继续"
                        }
                    }

                    button {
                        class: "ws-verify__submit",
                        r#type: "submit",
                        disabled: verify_disabled,
                        if *loading.read() {
                            "验证中..."
                        } else if locked {
                            "请先重新发送"
                        } else {
                            "验证"
                        }
                    }
                }

                div { class: "ws-verify__resend-row",
                    button {
                        class: "ws-verify__resend",
                        r#type: "button",
                        disabled: *loading.read() || *countdown.read() > 0,
                        onclick: on_resend,
                        "{resend_label}"
                    }
                    a {
                        class: "ws-verify__back",
                        href: "#",
                        onclick: move |e| {
                            e.prevent_default();
                            on_back();
                        },
                        "返回登录"
                    }
                }
            }
        }
    }
}
