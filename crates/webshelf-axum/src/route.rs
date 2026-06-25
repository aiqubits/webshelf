use axum::body::Body;
use axum::response::{IntoResponse, Response as AxumResponse};
use http::StatusCode;
use std::future::Future;
use webshelf_runtime::{HttpError, Response, ResponseBody};

/// 统一 Response 的 axum 包装器，实现 IntoResponse
pub struct UnifiedResponse(pub Response);

impl IntoResponse for UnifiedResponse {
    fn into_response(self) -> AxumResponse {
        response_to_axum(self.0)
    }
}

/// 统一 HttpError 的 axum 包装器，实现 IntoResponse
pub struct UnifiedError(pub HttpError);

impl IntoResponse for UnifiedError {
    fn into_response(self) -> AxumResponse {
        let resp: Response = self.0.into();
        response_to_axum(resp)
    }
}

impl From<HttpError> for UnifiedError {
    fn from(err: HttpError) -> Self {
        UnifiedError(err)
    }
}

/// 将统一 Response 转换为 axum Response
pub fn response_to_axum(mut resp: Response) -> AxumResponse {
    let status = resp.status();

    // Determine Content-Type: explicit override takes priority, then auto-detect
    let content_type = resp.content_type().unwrap_or_else(|| {
        if matches!(resp.body(), ResponseBody::Json(_)) {
            "application/json; charset=utf-8"
        } else {
            "text/plain; charset=utf-8"
        }
    });

    let mut builder = AxumResponse::builder()
        .status(status)
        .header("content-type", content_type);

    // Cookie 已统一存储在 headers 中（通过 set-cookie header），
    // 因此只需遍历 headers 即可同时处理普通头部和 cookie。
    // Content-Type 由上面的显式 `set_content_type()` + 自动检测逻辑管理，
    // 从 headers 中过滤掉以免与 auto-detected / explicit 值冲突。
    for (name, value) in resp.take_headers() {
        if name.as_str().eq_ignore_ascii_case("content-type") {
            continue;
        }
        builder = builder.header(name.as_str(), &value);
    }

    match resp.read_bytes() {
        Ok(bytes) => builder.body(Body::from(bytes)).unwrap_or_default(),
        Err(e) => {
            tracing::error!("Failed to serialize response body: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// 创建 GET 方法路由（接受统一 async handler）
/// 由于 axum 0.8 的 Handler 约束限制，使用 Arc 包装器来满足 trait bound。
pub fn get<H, F, S>(handler: H) -> axum::routing::MethodRouter<S>
where
    H: Fn(crate::UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send,
    S: Clone + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(handler);
    axum::routing::get(move |req: crate::UnifiedRequest| {
        let handler = handler.clone();
        async move {
            match (*handler)(req).await {
                Ok(resp) => Ok(UnifiedResponse(resp)),
                Err(err) => Err(UnifiedError(err)),
            }
        }
    })
}

/// 创建 POST 方法路由
pub fn post<H, F, S>(handler: H) -> axum::routing::MethodRouter<S>
where
    H: Fn(crate::UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send,
    S: Clone + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(handler);
    axum::routing::post(move |req: crate::UnifiedRequest| {
        let handler = handler.clone();
        async move {
            match (*handler)(req).await {
                Ok(resp) => Ok(UnifiedResponse(resp)),
                Err(err) => Err(UnifiedError(err)),
            }
        }
    })
}

/// 创建 PUT 方法路由
pub fn put<H, F, S>(handler: H) -> axum::routing::MethodRouter<S>
where
    H: Fn(crate::UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send,
    S: Clone + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(handler);
    axum::routing::put(move |req: crate::UnifiedRequest| {
        let handler = handler.clone();
        async move {
            match (*handler)(req).await {
                Ok(resp) => Ok(UnifiedResponse(resp)),
                Err(err) => Err(UnifiedError(err)),
            }
        }
    })
}

/// 创建 DELETE 方法路由
pub fn delete<H, F, S>(handler: H) -> axum::routing::MethodRouter<S>
where
    H: Fn(crate::UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send,
    S: Clone + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(handler);
    axum::routing::delete(move |req: crate::UnifiedRequest| {
        let handler = handler.clone();
        async move {
            match (*handler)(req).await {
                Ok(resp) => Ok(UnifiedResponse(resp)),
                Err(err) => Err(UnifiedError(err)),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use axum::response::Response as AxumResponse;
    use bytes::Bytes;
    use http::StatusCode;
    use http_body_util::BodyExt;
    use webshelf_runtime::{HttpError, Response};

    use super::response_to_axum;

    /// Helper: extract response body as bytes for assertions.
    async fn body_bytes(resp: AxumResponse) -> Bytes {
        let collected = resp.into_body().collect().await.unwrap();
        collected.to_bytes()
    }

    /// Helper: extract response body as JSON value.
    async fn body_json(resp: AxumResponse) -> serde_json::Value {
        let bytes = body_bytes(resp).await;
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn json_body_sets_content_type() {
        let mut resp = Response::new();
        resp.set_json_body(serde_json::json!("hello"));
        let axum_resp = response_to_axum(resp);
        assert_eq!(
            axum_resp.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn text_body_sets_text_plain() {
        let mut resp = Response::new();
        resp.set_text_body("hello world");
        let axum_resp = response_to_axum(resp);
        assert_eq!(
            axum_resp.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        let bytes = body_bytes(axum_resp).await;
        assert_eq!(&bytes[..], b"hello world");
    }

    #[tokio::test]
    async fn explicit_content_type_overrides_auto_detect() {
        let mut resp = Response::new();
        resp.set_json_body(serde_json::json!("hello"));
        resp.set_content_type("application/pdf");
        let axum_resp = response_to_axum(resp);
        assert_eq!(
            axum_resp.headers().get("content-type").unwrap(),
            "application/pdf"
        );
    }

    #[tokio::test]
    async fn custom_header_preserved() {
        let mut resp = Response::new();
        resp.insert_header("x-custom", "value");
        let axum_resp = response_to_axum(resp);
        assert_eq!(axum_resp.headers().get("x-custom").unwrap(), "value");
    }

    #[tokio::test]
    async fn content_type_header_filtered_from_headers() {
        let mut resp = Response::new();
        resp.set_json_body(serde_json::json!("hello"));
        // Insert a direct content-type header (should be filtered out)
        resp.insert_header("content-type", "text/html");
        let axum_resp = response_to_axum(resp);
        // The explicit content-type from auto-detect should win
        assert_eq!(
            axum_resp.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn set_cookie_is_preserved() {
        let mut resp = Response::new();
        let cookie = cookie::Cookie::new("session", "abc123");
        resp.set_cookie(cookie);
        let axum_resp = response_to_axum(resp);
        let cookie_header = axum_resp.headers().get("set-cookie").unwrap();
        assert!(
            cookie_header.to_str().unwrap().contains("session=abc123"),
            "set-cookie should contain session=abc123, got: {:?}",
            cookie_header
        );
    }

    #[tokio::test]
    async fn empty_body_returns_empty_bytes() {
        let resp = Response::new();
        let axum_resp = response_to_axum(resp);
        let bytes = body_bytes(axum_resp).await;
        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn status_code_is_preserved() {
        let resp = Response::with_status(StatusCode::CREATED);
        let axum_resp = response_to_axum(resp);
        assert_eq!(axum_resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn http_error_to_axum_response() {
        let err = HttpError::not_found("resource missing");
        let unified_resp: Response = err.into();
        let axum_resp = response_to_axum(unified_resp);
        assert_eq!(axum_resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            axum_resp.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
        let json = body_json(axum_resp).await;
        assert_eq!(json["error"], "not_found");
        assert_eq!(json["message"], "resource missing");
    }

    #[tokio::test]
    async fn multiple_cookies_all_preserved() {
        let mut resp = Response::new();
        resp.set_cookie(cookie::Cookie::new("a", "1"));
        resp.set_cookie(cookie::Cookie::new("b", "2"));
        let axum_resp = response_to_axum(resp);
        let headers: Vec<_> = axum_resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        assert!(
            headers.iter().any(|h| h.contains("a=1")),
            "Should contain a=1 cookie"
        );
        assert!(
            headers.iter().any(|h| h.contains("b=2")),
            "Should contain b=2 cookie"
        );
    }

    #[tokio::test]
    async fn remove_cookie_works() {
        let mut resp = Response::new();
        resp.remove_cookie("old_session");
        let axum_resp = response_to_axum(resp);
        let cookie_header = axum_resp.headers().get("set-cookie").unwrap();
        let value = cookie_header.to_str().unwrap();
        assert!(value.contains("old_session="), "Should clear old_session");
        assert!(value.contains("Max-Age=0"), "Should set Max-Age=0");
    }
}
