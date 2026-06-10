use crate::error::ClientError;

/// 客户端配置
///
/// 配置 HTTP 客户端的基本参数：基础 URL、超时时间、重试策略等。
///
/// # Examples
///
/// ```rust,no_run
/// use client_api::ClientConfig;
///
/// // 原生平台：指定后端地址
/// let config = ClientConfig::new("http://localhost:8080");
///
/// // 空 base_url 仅在 WASM（浏览器）下有效
/// // let config = ClientConfig::new("");
///
/// // Builder 模式自定义
/// let config = ClientConfig::new("http://localhost:8080")
///     .with_timeout(60)
///     .with_max_retries(5);
/// ```
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// API 基础 URL。空字符串表示使用相对路径（适用于 Nginx 反向代理场景）。
    pub base_url: String,
    /// 请求超时时间（秒）
    pub timeout_secs: u64,
    /// 最大重试次数。设置为 3 表示失败后最多额外重试 3 次，
    /// 即总共最多发起 4 次请求（1 次初始 + 3 次重试）。
    /// 设置为 0 表示禁用重试。
    pub max_retries: u32,
}

impl ClientConfig {
    /// 创建新的配置
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }

    /// 设置超时时间（秒）
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 设置最大重试次数（0 表示禁用重试）
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// 验证配置是否有效
    pub fn validate(&self) -> Result<(), ClientError> {
        // 原生平台需要绝对 URL（相对路径仅在 WASM 下通过 window.location 推导）
        #[cfg(not(target_arch = "wasm32"))]
        if self.base_url.is_empty() {
            return Err(ClientError::Config(
                "Base URL is required on native platform; use an absolute URL ".to_string(),
            ));
        }

        if !self.base_url.is_empty()
            && !self.base_url.starts_with("http://")
            && !self.base_url.starts_with("https://")
        {
            return Err(ClientError::Config(
                "Base URL must start with http:// or https://".to_string(),
            ));
        }

        if self.timeout_secs == 0 {
            return Err(ClientError::Config(
                "Timeout must be greater than 0".to_string(),
            ));
        }

        // WASM 下空 base_url 需要浏览器 window 上下文来推导 origin
        #[cfg(target_arch = "wasm32")]
        {
            if self.base_url.is_empty() && web_sys::window().is_none() {
                return Err(ClientError::Config(
                    "Empty base_url requires window context in WASM (not available in Web Workers)"
                        .to_string(),
                ));
            }
        }

        Ok(())
    }

    /// 构建完整 URL
    ///
    /// 空字符串 `base_url` 表示使用相对路径（适用于 Nginx 反向代理场景）。
    /// 在 WASM 环境下，空 `base_url` 会自动从浏览器 `window.location.origin()` 推导绝对 URL。
    pub fn build_url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');

        if self.base_url.is_empty() {
            // 相对路径模式：Nginx 反向代理 / WASM 同源请求
            #[cfg(target_arch = "wasm32")]
            {
                // WASM 下 reqwest 需要绝对 URL，从浏览器 location 推导
                if let Some(window) = web_sys::window() {
                    if let Ok(origin) = window.location().origin() {
                        return format!("{}/{}", origin.trim_end_matches('/'), path);
                    }
                }
            }
            // 回退：理论上不可达——validate() 在 WASM 上已确保 window 存在，
            // 且 origin() 在标准浏览器 API 中从不失败。保留此路径仅作为防御性编程。
            format!("/{}", path)
        } else {
            let base = self.base_url.trim_end_matches('/');
            format!("{}/{}", base, path)
        }
    }
}

impl Default for ClientConfig {
    /// 默认配置使用 `http://localhost:8080` 作为 base_url，
    /// 适用于本地开发环境。生产环境请通过 `ClientConfig::new()` 显式指定。
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        // 有效配置：HTTP
        let config = ClientConfig::new("http://localhost:8080");
        assert!(config.validate().is_ok());

        // 有效配置：HTTPS
        let config = ClientConfig::new("https://api.example.com");
        assert!(config.validate().is_ok());

        // 原生平台：空 URL 无效（相对路径仅在 WASM 下通过 window.location 推导）
        let config = ClientConfig::new("");
        assert!(config.validate().is_err());

        // 无效配置：错误协议
        let config = ClientConfig::new("ftp://localhost");
        assert!(config.validate().is_err());

        // 无效配置：超时为 0
        let config = ClientConfig::new("http://localhost:8080").with_timeout(0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_build_url() {
        // 正常 URL
        let config = ClientConfig::new("http://localhost:8080");
        assert_eq!(
            config.build_url("/api/v1/users"),
            "http://localhost:8080/api/v1/users"
        );
        assert_eq!(
            config.build_url("api/v1/users"),
            "http://localhost:8080/api/v1/users"
        );

        // 带尾部斜杠的 URL
        let config = ClientConfig::new("http://localhost:8080/");
        assert_eq!(
            config.build_url("/api/v1/users"),
            "http://localhost:8080/api/v1/users"
        );

        // 空字符串（相对路径）
        let config = ClientConfig::new("");
        assert_eq!(config.build_url("/api/v1/users"), "/api/v1/users");
        assert_eq!(config.build_url("api/v1/users"), "/api/v1/users");
    }

    #[test]
    fn test_builder_pattern() {
        let config = ClientConfig::new("http://localhost:8080")
            .with_timeout(60)
            .with_max_retries(5);

        assert_eq!(config.base_url, "http://localhost:8080");
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_default_config() {
        let config = ClientConfig::default();
        assert_eq!(config.base_url, "http://localhost:8080");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_retries, 3);
    }
}
