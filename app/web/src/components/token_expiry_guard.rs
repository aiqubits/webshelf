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
//!
//! ## 实现说明
//!
//! 使用 `use_resource` 而非 `use_effect + spawn`，确保
//! `token_expires_at` 变化或组件卸载时**旧计时器自动取消**，
//! 避免多个 timer 并行运行的资源浪费。

use dioxus::prelude::dioxus_router::Navigator;
use dioxus::prelude::*;

use crate::Route;
use crate::auth::AuthState;
use crate::auth::JWT_EXPIRY_LEEWAY_SECS;
use crate::components::{HttpMethod, LogBus, LogKind};

#[component]
pub fn TokenExpiryGuard() -> Element {
    let auth = use_context::<AuthState>();
    let nav = use_navigator();
    let log_bus = use_context::<LogBus>();

    let expires_at = auth.token_expires_at;

    // use_resource 在 expires_at 变化或组件卸载时自动取消旧异步任务，
    // 避免旧 timer 与新 timer 并存（use_effect + spawn 无法取消旧任务）。
    //
    // 闭包同步读取 `expires_at` 以建立信号追踪；异步块内执行计时与登出。
    use_resource(move || {
        let exp = expires_at.cloned();
        let auth_async = auth.clone();
        let nav_async = nav;
        let bus_async = log_bus;

        async move {
            let Some(exp_secs) = exp else {
                return;
            };
            let now = crate::components::now_unix_secs();

            if now + JWT_EXPIRY_LEEWAY_SECS >= exp_secs {
                fire_expiry(auth_async, nav_async, bus_async);
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

            fire_expiry(auth_async, nav_async, bus_async);
        }
    });

    rsx! {
        Fragment {}

    }
}

/// 触发过期登出。再次读取 `token_expires_at` 以避免 sleep 期间用户重新登录
/// 导致的误杀（新 token 尚未过期时直接返回）。
fn fire_expiry(mut auth: AuthState, nav: Navigator, mut log_bus: LogBus) {
    let now = crate::components::now_unix_secs();
    let still_expired = match auth.token_expires_at.cloned() {
        Some(exp) => now + JWT_EXPIRY_LEEWAY_SECS >= exp,
        None => false,
    };
    if !still_expired {
        return;
    }
    auth.logout();
    nav.replace(Route::Auth {});
    log_bus.push(
        HttpMethod::Post,
        "/auth/logout (token expired)".to_string(),
        "401".to_string(),
        LogKind::Important,
    );
}
