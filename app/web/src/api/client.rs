use client_api::{Client, ClientConfig, ClientError};

/// 根据编译目标构造 `client_api::Client`。
///
/// - **WASM（浏览器）**：使用空 `base_url`，请求会指向 `window.location.origin/api/...`，
///   通常配合 Nginx 反向代理同源部署。
/// - **Native（桌面 / 移动 / 测试）**：读取 `WEBSHELF_API_URL` 环境变量，
///   未设置则回退到 `http://127.0.0.1:8080`。
pub fn make_client() -> Result<Client, ClientError> {
    let config = client_config();
    Client::new(config)
}

fn client_config() -> ClientConfig {
    #[cfg(target_arch = "wasm32")]
    {
        // WASM 留空 base_url —— `client-api` 会通过 `window.location.origin` 推导。
        ClientConfig::new("")
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let base = std::env::var("WEBSHELF_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        ClientConfig::new(base)
    }
}
