//! 客户端 API 工厂与 401 拦截辅助。
//!
//! - `make_client()` 根据编译目标返回合适的 `client_api::Client`。
//! - `is_unauth(err)` 判定一个 `ClientError` 是否表示 token 失效。
//! - `handle_unauth(err, auth, nav)` 检测到 401 时执行 logout + 跳转 `/auth`。

mod client;

pub use client::make_client;

use client_api::ClientError;
use dioxus::prelude::dioxus_router::Navigator;

use crate::Route;
use crate::auth::AuthState;

/// 判定一个 `ClientError` 是否代表 token 失效（HTTP 401）。
pub fn is_unauth(err: &ClientError) -> bool {
    matches!(err, ClientError::Other(401, _))
}

/// 若 `err` 为 401，则调用 `auth.logout()` 并把路由切换到 `/auth`，返回 `true`；
/// 否则原样返回 `false`，让调用方继续处理业务错误。
///
/// 视图层模式：
/// ```ignore
/// if let Err(e) = client.list_users(1, 20).await {
///     if handle_unauth(&e, auth, nav) { return; }
///     // 处理业务错误...
/// }
/// ```
pub fn handle_unauth(err: &ClientError, mut auth: AuthState, nav: Navigator) -> bool {
    if is_unauth(err) {
        auth.logout();
        nav.push(Route::Auth {});
        true
    } else {
        false
    }
}
