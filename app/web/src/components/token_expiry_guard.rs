//! TokenExpiryGuard —— JWT 过期自动登出。
//!
//! 监听 `AuthState::token_expires_at` 信号；当到达 `exp` 时间时，
//! 自动调用 `auth.logout()`、推送 `/auth`，并向 `LogBus` 写入一条
//! `LogKind::Important` 提示，让 toast 通知用户。
//!
//! 这是 401 拦截器之外的第二道防线：
//! - 401 拦截器在「下一次 API 调用」时被动触发；
//! - 本组件主动在 `exp` 到达时立即把用户赶回登录页，
//!   避免用户在过期 token 下静默操作而引发混淆的错误。

use dioxus::prelude::dioxus_router::Navigator;
use dioxus::prelude::*;

use crate::Route;
use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, LogKind};

#[component]
pub fn TokenExpiryGuard() -> Element {
    let auth = use_context::<AuthState>();
    let nav = use_navigator();
    let log_bus = use_context::<LogBus>();

    // 每次 effect 触发时自增，用于让旧的 sleep 任务在醒来后识别出自己已过期。
    let mut guard_gen = use_signal(|| 0u64);
    let expires_at = auth.token_expires_at;

    use_effect(move || {
        let exp = expires_at.cloned();
        let Some(exp_secs) = exp else {
            return;
        };
        let now = crate::auth::now_unix_secs();
        // 抢锁：捕获当前 generation，并自增让后续 effect 拿到新值。
        let my_gen = *guard_gen.read();
        guard_gen.with_mut(|g| *g = g.wrapping_add(1));

        let auth_async = auth.clone();
        let nav_async = nav;
        let bus_async = log_bus;
        let gen_async = guard_gen;
        spawn(async move {
            if now >= exp_secs {
                fire_expiry(auth_async, nav_async, bus_async, gen_async, my_gen);
                return;
            }
            let delay_ms = (exp_secs - now).saturating_mul(1000);
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::future::TimeoutFuture::new(delay_ms.min(u32::MAX as u64) as u32).await;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            fire_expiry(auth_async, nav_async, bus_async, gen_async, my_gen);
        });
    });

    rsx! { Fragment {} }
}

/// 触发过期登出。Generation 不匹配（被新 effect 覆盖）时直接放弃，
/// 避免「调度时未过期 → 睡眠期间用户重新登录 → 醒来后误杀新会话」的竞态。
/// 由于这是 WASM 单线程事件循环，read-and-act 之间不会被打断。
fn fire_expiry(
    mut auth: AuthState,
    nav: Navigator,
    mut log_bus: LogBus,
    guard_gen: Signal<u64>,
    my_gen: u64,
) {
    // Generation 检查 —— 已被新 effect 覆盖则不动作。
    if *guard_gen.read() != my_gen {
        return;
    }
    let now = crate::auth::now_unix_secs();
    let still_expired = match auth.token_expires_at.cloned() {
        Some(exp) => now >= exp,
        None => false,
    };
    if !still_expired {
        return;
    }
    auth.logout();
    nav.push(Route::Auth {});
    log_bus.push(
        HttpMethod::Post,
        "/auth/logout (token expired)".to_string(),
        "401".to_string(),
        LogKind::Important,
    );
}
