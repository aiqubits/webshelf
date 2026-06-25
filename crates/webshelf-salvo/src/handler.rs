use std::future::Future;

use bytes::Bytes;
use salvo::http::StatusCode;
use salvo::{Depot, FlowCtrl, Handler, Request, Response};
use webshelf_runtime::{HttpError, Response as UnifiedResponse};

use crate::{UnifiedRequest, render_response};

/// Wrapper type for body bytes cached in Depot.
/// Used to bridge the gap between middlewares (RateLimitMiddleware) that consume
/// the request body for email extraction and UnifiedHandler that needs the body
/// for parsing. Middleware stores bytes here, handler reads them first before
/// calling `req.payload()`.
#[derive(Clone)]
pub(crate) struct CachedBody(pub Bytes);

/// Handler 包装器 —— 将统一风格的 async handler 包装为 salvo::Handler
///
/// 支持签名：`async fn(UnifiedRequest) -> Result<Response, HttpError>`
///
/// **Eager Buffering**：在 handle() 中 async 预读 body 后再创建 UnifiedRequest。
/// **GET/HEAD 优化**：无 body 的请求跳过 body 读取。
pub struct UnifiedHandler<H, F>(pub H)
where
    H: Fn(UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<UnifiedResponse, HttpError>> + Send;

#[async_trait::async_trait]
impl<H, F> Handler for UnifiedHandler<H, F>
where
    H: Fn(UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<UnifiedResponse, HttpError>> + Send + 'static,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        // ⚡ Eager Buffering：预读 body
        // 先检查 Depot 中是否有上游中间件缓存的 body（例如 RateLimitMiddleware
        // 在 email 限流时预读了 body），若无则从 req.payload() 读取。
        let cached_body = if req.method() == salvo::http::Method::GET
            || req.method() == salvo::http::Method::HEAD
        {
            Bytes::new()
        } else {
            let body_from_depot = depot.obtain::<CachedBody>().ok().map(|cb| cb.0.clone());
            match body_from_depot {
                // 只要 Depot 中有缓存 body 就使用它（无论是否为空），
                // 因为上游 RateLimitMiddleware 已消耗了请求 body。
                Some(bytes) => bytes,
                _ => {
                    // Fallback: 从 req.payload() 读取 body。
                    // 这在以下情况可能返回空：
                    // 1. 上游中间件消耗了 body 但未存储为 CachedBody（应修复）
                    // 2. GET/HEAD 请求（已在上层处理）
                    // 当 body 意外为空时记录警告以辅助调试。
                    match req.payload().await {
                        Ok(bytes) => {
                            let body = bytes.clone();
                            if body.is_empty() {
                                tracing::warn!(
                                    "UnifiedHandler: req.payload() returned empty body — an upstream middleware may have consumed it without caching as CachedBody"
                                );
                            }
                            body
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to read request body in UnifiedHandler: {:?}",
                                e
                            );
                            res.status_code(StatusCode::BAD_REQUEST);
                            res.render(salvo::writing::Json(serde_json::json!({
                                "error": "bad_request",
                                "message": "Failed to read request body"
                            })));
                            return;
                        }
                    }
                }
            }
        };
        // SAFETY: UnifiedRequest 的 req/depot 指针在此 handle() 调用期间有效
        let unified_req = unsafe { UnifiedRequest::new(req, depot, cached_body) };
        match (self.0)(unified_req).await {
            Ok(unified_res) => render_response(unified_res, res),
            Err(err) => {
                let err_res: UnifiedResponse = err.into();
                render_response(err_res, res);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use salvo::Depot;
    use std::sync::Arc;
    use webshelf_runtime::RequestContext;

    #[tokio::test]
    async fn unified_handler_uses_cached_body_from_depot() {
        async fn echo_body(mut req: UnifiedRequest) -> Result<UnifiedResponse, HttpError> {
            let body = req.read_body_bytes().await.map_err(HttpError::internal)?;
            let mut resp = UnifiedResponse::new();
            resp.set_bytes_body(body);
            Ok(resp)
        }

        let handler = UnifiedHandler(echo_body);

        // Create Depot with a cached body that differs from the request body.
        let mut depot = Depot::new();
        depot.inject(CachedBody(Bytes::from("from_depot")));

        // Build a salvo Request whose payload is different from the cached body.
        // If UnifiedHandler falls back to req.payload(), it would get "from_payload"
        // instead of "from_depot". The test proves it reads from Depot.
        let hyper_req = salvo::hyper::Request::builder()
            .method("POST")
            .header("content-type", "application/json")
            .body(salvo::http::body::ReqBody::Once(Bytes::from(
                "from_payload",
            )))
            .unwrap();
        let mut req = salvo::Request::new();
        req.merge_hyper(hyper_req);

        let mut res = salvo::Response::new();
        // FlowCtrl with no next handlers (handler is last in the chain).
        let mut ctrl = FlowCtrl::new(Vec::<Arc<dyn Handler>>::new());

        handler
            .handle(&mut req, &mut depot, &mut res, &mut ctrl)
            .await;

        // Handler should have succeeded — UnifiedResponse was rendered.
        // The handler read body via read_body_bytes() which returns cached_body
        // set up by UnifiedHandler. Since we put "from_depot" in Depot and
        // "from_payload" in the Request payload, if the handler returns
        // "from_depot" it proves the Depot path was taken.
        assert_eq!(res.status_code, Some(StatusCode::OK));

        // Verify the response body content — it MUST contain the Depot value,
        // NOT the request payload value.
        match &res.body {
            salvo::http::body::ResBody::Once(bytes) => {
                assert_eq!(
                    std::str::from_utf8(bytes).unwrap(),
                    "from_depot",
                    "Expected body from Depot but got body from request payload"
                );
            }
            _ => {
                panic!("Expected ResBody::Once, got different ResBody variant");
            }
        }
    }
}
