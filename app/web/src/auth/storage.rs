//! 浏览器 localStorage 包装 —— 用于持久化 JWT。
//!
//! 在非 WASM 目标下为 no-op，使 `cargo check -p web` 在 native 平台也能通过。

#[allow(dead_code)]
const TOKEN_KEY: &str = "webshelf.jwt";

/// 保存 JWT。失败（如 quota exceeded）静默忽略。
pub fn save_token(token: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window()
            && let Ok(Some(storage)) = window.local_storage()
        {
            let _ = storage.set_item(TOKEN_KEY, token);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = token;
    }
}

/// 读取 JWT。若无、解析失败、localStorage 不可用，均返回 `None`。
pub fn load_token() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        storage.get_item(TOKEN_KEY).ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// 清除 JWT。
pub fn clear_token() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window()
            && let Ok(Some(storage)) = window.local_storage()
        {
            let _ = storage.remove_item(TOKEN_KEY);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // no-op
    }
}
