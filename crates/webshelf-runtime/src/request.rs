use bytes::Bytes;
use serde::de::DeserializeOwned;
use std::net::IpAddr;
use std::str::FromStr;

/// Unified request context — each adapter implements this on its native Request type.
/// Body parsing methods (parse_json / parse_form / read_body_bytes) are safe to call multiple times.
#[allow(async_fn_in_trait)]
pub trait RequestContext: Send + Sync {
    /// HTTP 方法（如 "GET", "POST"）
    fn method(&self) -> &str;

    /// 请求路径
    fn path(&self) -> &str;

    /// 客户端 IP
    fn client_ip(&self) -> Option<IpAddr>;

    /// 获取单个请求头
    fn header(&self, name: &str) -> Option<&str>;

    /// Return matched route pattern (e.g. `/users/{id}`) for path parameter extraction.
    ///
    /// **⚠️ 维护警告**：默认的 `get_param()` 实现依赖此方法进行路径参数提取。
    /// Salvo 适配器端已重写 `get_param()` 以使用 salvo 原生 `req.param(name)`，
    /// 并在此处返回 `None`。**任何在此 trait 上新增的使用 `matched_route_pattern()`
    /// 的默认方法，都必须在 Salvo 适配器端同步重写**，否则会导致参数提取静默失败。
    fn matched_route_pattern(&self) -> Option<&str>;

    fn get_param(&self, name: &str) -> Option<&str> {
        let pattern = self.matched_route_pattern()?;
        let path = self.path();
        let pattern_segs: Vec<&str> = pattern.trim_matches('/').split('/').collect();
        let path_segs: Vec<&str> = path.trim_matches('/').split('/').collect();
        for (p_seg, a_seg) in pattern_segs.iter().zip(path_segs.iter()) {
            if let Some(param_name) = p_seg.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
                && param_name == name
            {
                return Some(a_seg);
            }
        }
        None
    }

    /// 解析路径参数为目标类型
    fn parse_param<T: FromStr>(&self, name: &str) -> Result<T, String> {
        self.get_param(name)
            .ok_or_else(|| format!("param `{name}` not found"))?
            .parse::<T>()
            .map_err(|_| format!("failed to parse param `{name}`"))
    }

    /// Parse query string
    fn parse_query<T: DeserializeOwned>(&self) -> Result<T, String>;

    /// Parse request body as JSON
    async fn parse_json<T: DeserializeOwned>(&mut self) -> Result<T, String>;

    /// Parse request body as form data
    async fn parse_form<T: DeserializeOwned>(&mut self) -> Result<T, String>;

    /// Read raw request body bytes
    async fn read_body_bytes(&mut self) -> Result<Bytes, String>;

    /// Auto-select JSON or form parsing based on Content-Type
    async fn parse_json_or_form<T: DeserializeOwned>(&mut self) -> Result<T, String> {
        let content_type = self
            .header("content-type")
            .unwrap_or("")
            .to_ascii_lowercase();
        if content_type.contains("application/x-www-form-urlencoded") {
            self.parse_form().await
        } else {
            self.parse_json().await
        }
    }

    /// Get request-scoped injected data (e.g. AppState, AuthUser)
    fn get_data<T: Clone + Send + Sync + 'static>(&self) -> Option<T>;

    /// Get reference to request-scoped data (zero-overhead, avoids Clone on hot paths)
    fn get_data_ref<T: Send + Sync + 'static>(&self) -> Option<&T>;

    /// Inject request-scoped data
    fn set_data<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Option<T>;

    /// Read a request cookie
    fn cookie(&self, name: &str) -> Option<String>;
}
