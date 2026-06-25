use axum::{
    Json,
    extract::{FromRequest, MatchedPath, Request},
    http::request::Parts,
    http::{Method, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::net::IpAddr;

use webshelf_runtime::RequestContext;

/// Unified request (Axum) — body is eagerly buffered in `FromRequest`.
pub struct UnifiedRequest {
    parts: Parts,
    cached_body: Bytes,
}

impl UnifiedRequest {
    pub fn new(parts: Parts, cached_body: Bytes) -> Self {
        Self { parts, cached_body }
    }
}

/// Implements axum::FromRequest — eagerly buffers body and injects router state into extensions.
impl<S> FromRequest<S> for UnifiedRequest
where
    S: Clone + Send + Sync + 'static,
{
    type Rejection = (http::StatusCode, Json<serde_json::Value>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (mut parts, body) = req.into_parts();
        parts.extensions.insert(state.clone());

        // GET/HEAD 请求通常无 body，跳过 eager buffering 以节省不必要的 I/O。
        // 若有意外附带 body，其内容将被忽略（与 HTTP 语义一致）。
        let bytes = if parts.method == Method::GET || parts.method == Method::HEAD {
            Bytes::new()
        } else {
            body.collect()
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "bad_request", "message": e.to_string()})),
                    )
                })?
                .to_bytes()
        };

        Ok(Self {
            parts,
            cached_body: bytes,
        })
    }
}

impl RequestContext for UnifiedRequest {
    fn method(&self) -> &str {
        self.parts.method.as_str()
    }

    fn path(&self) -> &str {
        self.parts.uri.path()
    }

    fn client_ip(&self) -> Option<IpAddr> {
        // Check X-Forwarded-For header first (proxy / load balancer support),
        // then X-Real-IP, then fall back to peer IP from ConnectInfo extension.
        if let Some(ip) = self
            .header("x-forwarded-for")
            .and_then(|v| v.split(',').next().map(|s| s.trim()))
            .and_then(|s| s.parse::<IpAddr>().ok())
        {
            return Some(ip);
        }

        if let Some(ip) = self
            .header("x-real-ip")
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
        {
            return Some(ip);
        }

        self.parts
            .extensions
            .get::<std::net::SocketAddr>()
            .map(|addr| addr.ip())
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.parts.headers.get(name).and_then(|v| v.to_str().ok())
    }

    fn matched_route_pattern(&self) -> Option<&str> {
        self.parts
            .extensions
            .get::<MatchedPath>()
            .map(|mp| mp.as_str())
    }

    /// Override default get_param to handle axum nest path prefix stripping.
    fn get_param(&self, name: &str) -> Option<&str> {
        let pattern = self.matched_route_pattern()?;
        let path = self.path();
        let pattern_segs: Vec<&str> = pattern.trim_matches('/').split('/').collect();
        let path_segs: Vec<&str> = path.trim_matches('/').split('/').collect();

        let offset = pattern_segs.len().saturating_sub(path_segs.len());
        for (p_seg, a_seg) in pattern_segs[offset..].iter().zip(path_segs.iter()) {
            if let Some(param_name) = p_seg.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
                && param_name == name
            {
                return Some(a_seg);
            }
        }
        None
    }

    fn parse_query<T: DeserializeOwned>(&self) -> Result<T, String> {
        let query_str = self.parts.uri.query().unwrap_or("");
        serde_urlencoded::from_str(query_str).map_err(|e| e.to_string())
    }

    fn get_data<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        self.parts.extensions.get::<T>().cloned()
    }

    fn get_data_ref<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.parts.extensions.get::<T>()
    }

    fn set_data<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.parts.extensions.insert(value)
    }

    fn cookie(&self, name: &str) -> Option<String> {
        let cookie_str = self.parts.headers.get("cookie")?.to_str().ok()?;
        cookie_str
            .split(';')
            .map(str::trim)
            .filter_map(|c| cookie::Cookie::parse(c).ok())
            .find(|c| c.name() == name)
            .map(|c| c.value().to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    async fn make_unified_request(header_name: &str, header_value: &str) -> UnifiedRequest {
        let req = Request::builder()
            .header(header_name, header_value)
            .body(Body::empty())
            .unwrap();
        let (parts, body) = req.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        UnifiedRequest::new(parts, bytes)
    }

    #[tokio::test]
    async fn client_ip_from_x_forwarded_for() {
        let req = make_unified_request("x-forwarded-for", "203.0.113.1").await;
        assert_eq!(
            req.client_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)))
        );
    }

    #[tokio::test]
    async fn client_ip_from_x_forwarded_for_multiple() {
        // X-Forwarded-For may contain a chain of proxies; we take the leftmost.
        let req = make_unified_request("x-forwarded-for", "198.51.100.1, 10.0.0.1").await;
        assert_eq!(
            req.client_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1)))
        );
    }

    #[tokio::test]
    async fn client_ip_from_x_real_ip() {
        let req = make_unified_request("x-real-ip", "192.168.1.42").await;
        assert_eq!(
            req.client_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)))
        );
    }

    #[tokio::test]
    async fn client_ip_x_forwarded_for_takes_priority() {
        // When both headers are present, X-Forwarded-For wins.
        let req = Request::builder()
            .header("x-forwarded-for", "10.0.0.1")
            .header("x-real-ip", "192.168.1.100")
            .body(Body::empty())
            .unwrap();
        let (parts, body) = req.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let unified = UnifiedRequest::new(parts, bytes);
        assert_eq!(
            unified.client_ip(),
            Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)))
        );
    }

    #[tokio::test]
    async fn client_ip_x_forwarded_for_ipv6() {
        let req = make_unified_request("x-forwarded-for", "::1").await;
        assert_eq!(
            req.client_ip(),
            Some(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
        );
    }

    #[tokio::test]
    async fn client_ip_no_headers_returns_none() {
        // Without ConnectInfo in extensions and without proxy headers, client_ip is None.
        let req = Request::builder().body(Body::empty()).unwrap();
        let (parts, body) = req.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let unified = UnifiedRequest::new(parts, bytes);
        assert!(unified.client_ip().is_none());
    }

    #[tokio::test]
    async fn client_ip_invalid_x_forwarded_for_ignored() {
        // Invalid IP in X-Forwarded-For should be ignored (falls through to None).
        let req = make_unified_request("x-forwarded-for", "not-an-ip").await;
        assert!(req.client_ip().is_none());
    }

    // ── set_data contract tests ─────────────────────────────────

    #[tokio::test]
    async fn set_data_returns_none_on_first_call() {
        let req = Request::builder().body(Body::empty()).unwrap();
        let (parts, body) = req.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let mut unified = UnifiedRequest::new(parts, bytes);

        // First set should return None (no previous value)
        let old = unified.set_data(42i32);
        assert!(old.is_none(), "first set_data should return None");
    }

    #[tokio::test]
    async fn set_data_returns_previous_value_on_overwrite() {
        let req = Request::builder().body(Body::empty()).unwrap();
        let (parts, body) = req.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let mut unified = UnifiedRequest::new(parts, bytes);

        unified.set_data("first");
        let old = unified.set_data("second");

        assert_eq!(
            old,
            Some("first"),
            "set_data should return the previously stored value"
        );
    }

    #[tokio::test]
    async fn set_data_different_types_independent() {
        let req = Request::builder().body(Body::empty()).unwrap();
        let (parts, body) = req.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let mut unified = UnifiedRequest::new(parts, bytes);

        unified.set_data(42i32);
        // A different type has its own slot — should return None
        let old = unified.set_data("hello");
        assert!(old.is_none(), "different type should have independent slot");
    }

    #[tokio::test]
    async fn set_data_and_get_data_roundtrip() {
        let req = Request::builder().body(Body::empty()).unwrap();
        let (parts, body) = req.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let mut unified = UnifiedRequest::new(parts, bytes);

        unified.set_data("stored_value");
        let retrieved: Option<&str> = unified.get_data();
        assert_eq!(
            retrieved,
            Some("stored_value"),
            "get_data should retrieve the stored value"
        );
    }
}
