//! TokenExpiryGuard —— JWT 过期自动登出（带静默刷新）。
//!
//! 监听 `AuthState::token_expires_at` 信号；当到达 `exp` 时间时，
//! 先尝试静默刷新（`try_refresh_async`）；刷新失败才调用 `auth.logout()`、
//! 推送 `/auth`，并向 `LogBus` 写入一条 `LogKind::Important` 提示。
//!
//! 这是 401 拦截器之外的第二道防线：
//! - 401 拦截器在「下一次 API 调用」时被动触发；
//! - 本组件主动在 `exp` 到达时尝试刷新，刷新失败才把用户赶回登录页。

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
                fire_expiry(auth_async, nav_async, bus_async).await;
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

            fire_expiry(auth_async, nav_async, bus_async).await;
        }
    });

    rsx! {
        Fragment {}
    }
}

/// 触发过期处理：先尝试静默刷新，失败才登出。
async fn fire_expiry(mut auth: AuthState, nav: Navigator, mut log_bus: LogBus) {
    let now = crate::components::now_unix_secs();
    let still_expired = match auth.token_expires_at.cloned() {
        Some(exp) => now + JWT_EXPIRY_LEEWAY_SECS >= exp,
        None => false,
    };
    if !still_expired {
        return;
    }

    // 尝试静默刷新
    if auth.try_refresh_async().await {
        // 刷新成功，用户会话已续期，无需登出
        return;
    }

    // 刷新失败，执行登出（撤销 refresh token + 清理本地状态）
    auth.logout_async().await;
    nav.replace(Route::LoginLanding {});
    log_bus.push(
        HttpMethod::Post,
        "/auth/logout (token expired, refresh failed)".to_string(),
        "401".to_string(),
        LogKind::Important,
    );
}
