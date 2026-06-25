use bytes::Bytes;
use salvo::Depot;
use salvo::http::Request as SalvoRequest;
use serde::de::DeserializeOwned;
use webshelf_runtime::RequestContext;

/// 统一请求上下文（Salvo 端）
///
/// # 安全性论证
/// `UnifiedRequest` 使用裸指针持有对 `SalvoRequest` 和 `Depot` 的引用。
/// 裸指针是必要的，因为 `UnifiedRequest` 需要实现 `Send` + `Sync` 以在 async 上下文中传递，
/// 而 Rust 的借用检查器无法证明跨 `.await` 点的借用有效性。
///
/// ## 安全不变式（Soundness Invariant）
/// 1. `UnifiedRequest` 仅在 `UnifiedHandler::handle()` 内部构造，
///    且在同一调用栈帧内同步使用，不会逃逸到其他任务或被 `tokio::spawn`。
/// 2. `handle(&self, req: &mut Request, depot: &mut Depot, ...)` 的参数 `req`/`depot`
///    的生命周期覆盖整个 `handle()` 调用（包括 `.await` 点）。
/// 3. Body 已被 Eager Buffered 为 `cached_body: Bytes`，`UnifiedRequest` 的所有
///    方法仅通过裸指针读取（不写入）`req`/`depot`。
/// 4. `cached_body` 是 owned `Bytes`，满足 `Send` + `Sync`。
///
/// # 维护警告：新增 RequestContext 方法时的同步义务
///
/// 当向 `webshelf_runtime::RequestContext` trait 添加**新的默认方法**时，
/// 必须在此适配器中检查是否需要重写（override），具体场景包括但不限于：
///
/// - 新方法使用了 `matched_route_pattern()`（Salvo 端返回 None，需自行实现）
/// - 新方法通过 `get_data()` / `get_data_ref()` 访问扩展数据（Salvo 端在 Depot 中）
/// - 新方法涉及 body 或 cookie 解析（Salvo 端通过裸指针访问原生类型）
///
/// **参考案例**：`get_param()` 的默认实现依赖 `matched_route_pattern()`，
/// Salvo 端已重写以使用原生 `req.param(name)` API。若新增的默认方法
/// 也依赖 `matched_route_pattern()`，必须同步重写，否则会导致参数
/// 提取在 Salvo 模式下静默失败。
#[must_use]
pub struct UnifiedRequest {
    req: std::ptr::NonNull<SalvoRequest>,
    depot: std::ptr::NonNull<Depot>,
    cached_body: Bytes,
}

// SAFETY: 见 struct 上的安全性论证。
unsafe impl Send for UnifiedRequest {}
unsafe impl Sync for UnifiedRequest {}

impl UnifiedRequest {
    /// 创建新的 UnifiedRequest。
    ///
    /// # Safety
    /// 调用者必须确保：
    /// - `req` 和 `depot` 的引用在 `UnifiedRequest` 的整个生命周期内保持有效
    /// - `UnifiedRequest` 不会逃逸当前 `handle()` 调用
    pub unsafe fn new(req: &mut SalvoRequest, depot: &mut Depot, cached_body: Bytes) -> Self {
        Self {
            req: std::ptr::NonNull::from(req),
            depot: std::ptr::NonNull::from(depot),
            cached_body,
        }
    }
}

impl RequestContext for UnifiedRequest {
    fn method(&self) -> &str {
        unsafe { (*self.req.as_ptr()).method().as_str() }
    }

    fn path(&self) -> &str {
        unsafe { (*self.req.as_ptr()).uri().path() }
    }

    fn client_ip(&self) -> Option<std::net::IpAddr> {
        // Check X-Forwarded-For header first (proxy / load balancer support)
        if let Some(ip) = self
            .header("x-forwarded-for")
            .and_then(|v| v.split(',').next().map(|s| s.trim()))
            .and_then(|s| s.parse::<std::net::IpAddr>().ok())
        {
            return Some(ip);
        }

        // Then X-Real-IP
        if let Some(ip) = self
            .header("x-real-ip")
            .and_then(|v| v.trim().parse::<std::net::IpAddr>().ok())
        {
            return Some(ip);
        }

        // Fall back to peer IP
        unsafe {
            match (*self.req.as_ptr()).remote_addr() {
                salvo::conn::SocketAddr::IPv4(addr) => Some(std::net::IpAddr::V4(*addr.ip())),
                salvo::conn::SocketAddr::IPv6(addr) => Some(std::net::IpAddr::V6(*addr.ip())),
                _ => None,
            }
        }
    }

    fn header(&self, name: &str) -> Option<&str> {
        unsafe { (*self.req.as_ptr()).headers().get(name)?.to_str().ok() }
    }

    fn matched_route_pattern(&self) -> Option<&str> {
        // ⚠️  IMPORTANT — 此方法故意返回 None。
        //
        // Salvo 通过原生 req.param(name) 提取路径参数（见 get_param() 的 override），
        // 不需要 matched_route_pattern 来做路径模板匹配。这与 Axum 不同（Axum 使用
        // MatchedPath extension 来获取路径模板）。
        //
        // RequestContext trait 的默认 get_param() 实现依赖 matched_route_pattern()。
        // Salvo 端已重写 get_param() 以避免使用默认实现（见上文）。任何在 trait 上
        // 新增的、使用 matched_route_pattern() 的默认方法，都必须在 Salvo 端同步重写。
        None
    }

    fn get_param(&self, name: &str) -> Option<&str> {
        unsafe { (*self.req.as_ptr()).param(name) }
    }

    fn parse_query<T: DeserializeOwned>(&self) -> Result<T, String> {
        let query_str = unsafe { (*self.req.as_ptr()).uri().query().unwrap_or("") };
        serde_urlencoded::from_str(query_str).map_err(|e| e.to_string())
    }

    fn get_data<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        unsafe { (*self.depot.as_ptr()).obtain::<T>().ok() }.cloned()
    }

    fn get_data_ref<T: Send + Sync + 'static>(&self) -> Option<&T> {
        unsafe { (*self.depot.as_ptr()).obtain::<T>().ok() }
    }

    fn set_data<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        unsafe {
            let old = (*self.depot.as_ptr()).scrape::<T>().ok();
            (*self.depot.as_ptr()).inject(value);
            old
        }
    }

    fn cookie(&self, name: &str) -> Option<String> {
        unsafe {
            let req = &*self.req.as_ptr();
            req.headers()
                .get("cookie")?
                .to_str()
                .ok()?
                .split(';')
                .filter_map(|c| cookie::Cookie::parse(c.trim()).ok())
                .find(|c| c.name() == name)
                .map(|c| c.value().to_string())
        }
    }

    async fn parse_json<T: DeserializeOwned>(&mut self) -> Result<T, String> {
        serde_json::from_slice(&self.cached_body).map_err(|e| e.to_string())
    }

    async fn read_body_bytes(&mut self) -> Result<Bytes, String> {
        Ok(self.cached_body.clone())
    }

    async fn parse_form<T: DeserializeOwned>(&mut self) -> Result<T, String> {
        serde_urlencoded::from_bytes(&self.cached_body).map_err(|e| e.to_string())
    }
}
