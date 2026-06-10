/// 客户端错误类型
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// 配置错误（如无效的 Base URL、超时参数）
    #[error("Configuration error: {0}")]
    Config(String),

    /// 网络/连接错误
    #[error("Network error: {0}")]
    Network(String),

    /// 服务器返回 5xx 错误
    #[error("Server error HTTP {0}: {1}")]
    ServerError(u16, String),

    /// 限流错误（HTTP 429）
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// JSON 反序列化错误（如响应体格式不匹配、字段缺失）
    ///
    /// 此类错误**不会触发重试**——因为每次重试返回的响应体完全相同，
    /// 重试无意义。
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// 其他 HTTP 错误（4xx 客户端错误等）
    #[error("HTTP {0}: {1}")]
    Other(u16, String),
}

impl ClientError {
    /// 根据 HTTP 状态码和响应体创建对应的错误类型
    pub fn from_status(status: u16, body: String) -> Self {
        match status {
            429 => Self::RateLimited(body),
            500..=599 => Self::ServerError(status, body),
            _ => Self::Other(status, body),
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            return Self::Network("Request timed out".to_string());
        }
        // `is_connect()` 仅在原生平台可用；WASM 下浏览器 fetch API
        // 所有网络层错误统一由 reqwest 以通用错误形式报告。
        #[cfg(not(target_arch = "wasm32"))]
        if e.is_connect() {
            return Self::Network(format!("Connection failed: {}", e));
        }
        if e.is_decode() {
            // reqwest 将 JSON 反序列化失败包装为 decode 错误，
            // 不应重试——响应体每次相同，必然反复失败
            Self::Deserialization(format!("Failed to decode response: {}", e))
        } else {
            Self::Network(e.to_string())
        }
    }
}

impl From<serde_json::Error> for ClientError {
    /// JSON 反序列化失败映射为 `Deserialization`，**不会**触发重试。
    ///
    /// 注意：`reqwest::Response::json::<T>()` 内部会在 serde 解析失败时
    /// 返回 `reqwest::Error`（包装了 `serde_json::Error`），因此这个转换
    /// 仅在直接使用 `serde_json` 的上下文中被触发。
    fn from(e: serde_json::Error) -> Self {
        Self::Deserialization(format!("Failed to parse response: {}", e))
    }
}

/// 服务端返回的结构化错误体
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ErrorBody {
    pub error: String,
    pub message: String,
}
