//! 浏览器 Cookie 包装 —— 用于持久化 JWT 过期时间。
//!
//! JWT 和 refresh token 由后端通过 httpOnly cookie 下发（`webshelf_jwt`、
//! `webshelf_refresh`），浏览器自动管理，前端 JS 无法读取。
//!
//! 前端仅维护一个可读的 `webshelf_exp` cookie，存储 JWT 过期时间
//! （Unix 秒字符串），用于 UI 层的过期检测和自动刷新决策。
//!
//! 在非 WASM 目标下为 no-op，使 `cargo check -p web` 在 native 平台也能通过。

/// 可读的 JWT 过期时间 cookie 名称。
///
/// 值为 Unix 秒字符串（如 "1719500000"）。
/// 由后端 login/refresh 时通过 `Set-Cookie` 设置，
/// 前端通过 `document.cookie` 读取以判断是否需要刷新。
#[allow(dead_code)]
const EXPIRY_COOKIE: &str = "webshelf_exp";

/// 保存 JWT 过期时间到可读 cookie。
///
/// `expires_at` 是 Unix 秒时间戳（JWT 的绝对过期时间）。
/// `max_age` 是 cookie 在浏览器中的最大存活秒数，应设为
/// 至少 `jwt_remember_expiry_seconds`（30天）或更长（如
/// `refresh_token_expiry_seconds` 90天），以确保前端在 JWT
/// 过期后仍能读取此 cookie 并触发静默刷新。
pub fn save_token(expires_at: u64, max_age: u64) {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::wasm_bindgen::JsCast;
        if let Some(window) = web_sys::window()
            && let Some(doc) = window.document()
            && let Some(html_doc) = doc.dyn_into::<web_sys::HtmlDocument>().ok()
        {
            let cookie = format!(
                "{}={}; Path=/; Max-Age={}; SameSite=Strict",
                EXPIRY_COOKIE, expires_at, max_age
            );
            let _ = html_doc.set_cookie(&cookie);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (expires_at, max_age);
    }
}

/// 读取 JWT 过期时间。若无、解析失败、cookie 不可用，均返回 `None`。
pub fn load_token() -> Option<u64> {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::wasm_bindgen::JsCast;
        let window = web_sys::window()?;
        let doc = window.document()?;
        let html_doc = doc.dyn_into::<web_sys::HtmlDocument>().ok()?;
        let cookies = html_doc.cookie().ok()?;

        cookies.split(';').find_map(|c| {
            let c = c.trim();
            if let Some(value) = c.strip_prefix(&format!("{}=", EXPIRY_COOKIE)) {
                value.parse::<u64>().ok()
            } else {
                None
            }
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// 清除 JWT 过期时间 cookie（设置 Max-Age=0）。
pub fn clear_token() {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::wasm_bindgen::JsCast;
        if let Some(window) = web_sys::window()
            && let Some(doc) = window.document()
            && let Some(html_doc) = doc.dyn_into::<web_sys::HtmlDocument>().ok()
        {
            let cookie = format!("{}=; Path=/; Max-Age=0; SameSite=Strict", EXPIRY_COOKIE);
            let _ = html_doc.set_cookie(&cookie);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // no-op
    }
}
