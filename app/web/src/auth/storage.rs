//! 浏览器持久化 —— JWT + 过期时间存储。
//!
//! JWT token 通过 sessionStorage 持久化，供页面刷新后通过 Authorization 头恢复；
//! JWT 过期时间通过 `webshelf_exp` cookie 存储，用于 UI 层的过期检测和刷新时机判断。
//! httpOnly cookie 由后端通过 `Set-Cookie` 下发，作为可选的第二认证通道。
//!
//! 在非 WASM 目标下为 no-op，使 `cargo check -p web` 在 native 平台也能通过。

/// sessionStorage 中 JWT token 的键名。
#[allow(dead_code)]
const JWT_STORAGE_KEY: &str = "webshelf_jwt";

/// 可读的 JWT 过期时间 cookie 名称。
///
/// 值为 Unix 秒字符串（如 "1719500000"）。
/// 由后端 login/refresh 时通过 `Set-Cookie` 设置，
/// 前端通过 `document.cookie` 读取以判断是否需要刷新。
#[allow(dead_code)]
const EXPIRY_COOKIE: &str = "webshelf_exp";

// ── JWT token (sessionStorage) ────────────────────────

/// 保存 JWT token 到 sessionStorage（页面刷新后恢复，关闭标签页后清除）。
pub fn save_jwt(token: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.session_storage() {
                let _ = storage.set_item(JWT_STORAGE_KEY, token);
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = token;
    }
}

/// 从 sessionStorage 读取 JWT token。若无或不可用则返回 `None`。
pub fn load_jwt() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window()?;
        if let Ok(Some(storage)) = window.session_storage() {
            storage.get_item(JWT_STORAGE_KEY).ok()?
        } else {
            None
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// 从 sessionStorage 中清除 JWT token。
pub fn clear_jwt() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.session_storage() {
                let _ = storage.remove_item(JWT_STORAGE_KEY);
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // no-op
    }
}

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

/// 清除全部持久化会话数据（expiry cookie + sessionStorage JWT）。
///
/// 同时清理 `webshelf_exp` cookie 和 sessionStorage 中的 JWT token，
/// 调用方无需再单独调用 `clear_jwt()`。
pub fn clear_token() {
    clear_jwt();
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
