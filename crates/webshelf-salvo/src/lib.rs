mod handler;
pub mod middleware;
mod render_response;
mod request;
mod route;
mod router;
mod runtime;

pub use router::SalvoRouter;
pub use runtime::SalvoRuntime;

// 统一 Request/Response/Handler 适配
pub use handler::UnifiedHandler;
pub use middleware::with_rate_limit_hoop;
pub use render_response::render_response;
pub use request::UnifiedRequest;
pub use route::{delete, get, post, put};

// ── Re-export salvo 生态类型 ───────────────────────

// Router — salvo 核心路由类型（原生类型，供 handler 使用）
// 注意：Runtime::Router 使用 SalvoRouter（Arc 包装版），
//       salvo::Router 原生类型仍可直接使用。
pub use salvo::Router;

// HTTP 核心类型 — 通过 salvo::http 提供
pub use salvo::http::header;
pub use salvo::http::{Method, StatusCode};

// Request / Response / Depot
pub use salvo::{Depot, Request, Response};

// Writer trait — 将数据写入 Response
pub use salvo::Writer;

// 中间件基础设施
pub use salvo::FlowCtrl;

// Handler trait — 手动实现 handler 的核心接口
pub use salvo::Handler;

// writing 模块 — 提供 writing::Json(...) 等响应写入工具
pub use salvo::writing;

// Salvo 预导入的常用类型
pub use salvo::catcher::Catcher;

#[cfg(test)]
mod tests {
    use crate::SalvoRouter;
    use crate::SalvoRuntime;
    use webshelf_runtime::Runtime;

    /// Test state type for SalvoRuntime verification.
    #[derive(Clone)]
    struct TestState;

    // ── Trait implementation check ──────────────────────────────

    #[test]
    fn salvo_runtime_implements_runtime_trait() {
        fn assert_impl_runtime<T: Runtime>() {}
        assert_impl_runtime::<SalvoRuntime<TestState>>();
    }

    #[test]
    fn unified_request_implements_request_context() {
        use webshelf_runtime::RequestContext;
        fn assert_impl_request_context<T: RequestContext>() {}
        assert_impl_request_context::<crate::UnifiedRequest>();
    }

    // ── Re-export compilation checks ────────────────────────────

    /// Verify that all core types are re-exported and usable.
    #[test]
    fn core_types_are_accessible() {
        // Router types
        fn _r(_: crate::Router) {}
        fn _sr(_: crate::SalvoRouter) {}

        // Response types
        fn _res(_: crate::Response) {}
        fn _dep(_: crate::Depot) {}

        // HTTP primitives
        fn _sc(_: crate::StatusCode) {}
        fn _m(_: crate::Method) {}

        // Modules — verify the re-exported module paths compile
        fn _hdr() {
            let _ = crate::header::SET_COOKIE;
        }
    }

    /// Verify Writer trait is accessible.
    #[test]
    fn writer_is_accessible() {
        fn _check<T: crate::Writer>() {}
    }

    /// Verify Handler trait is accessible.
    #[test]
    fn handler_is_accessible() {
        fn _check<T: crate::Handler>() {}
    }

    /// Verify writing module is accessible (e.g. writing::Json).
    #[test]
    fn writing_is_accessible() {
        let _json = crate::writing::Json(());
    }

    /// Verify routing free functions exist (unified).
    #[test]
    fn unified_routing_functions_are_accessible() {
        use webshelf_runtime::{HttpError, Response};

        async fn handler(_req: crate::UnifiedRequest) -> Result<Response, HttpError> {
            Ok(Response::new())
        }

        let _get = crate::get(handler);
        let _post = crate::post(handler);
        let _put = crate::put(handler);
        let _delete = crate::delete(handler);
    }

    // ── Router operation tests ──────────────────────────────────

    #[test]
    fn salvo_runtime_router_operations() {
        use webshelf_runtime::{HttpError, Response};

        async fn world_handler(_req: crate::UnifiedRequest) -> Result<Response, HttpError> {
            let mut resp = Response::new();
            resp.set_text_body("world");
            Ok(resp)
        }

        type R = SalvoRuntime<TestState>;

        let router = R::new_router();
        let merged = R::merge(router, R::new_router());
        let routed = R::with_route(merged, "/hello", crate::get(world_handler));
        let _nested = R::nest(routed, "/api", R::new_router());
    }

    #[test]
    fn salvo_runtime_with_state_compiles() {
        type R = SalvoRuntime<TestState>;

        let router = R::new_router();
        let state = TestState;
        let _with_state = R::with_state(router, state);
    }

    // ── Serve future Send check ─────────────────────────────────

    #[test]
    fn salvo_runtime_serve_returns_send_future() {
        let fut = SalvoRuntime::<TestState>::serve(SalvoRouter::new(), TestState, "0.0.0.0:0");
        fn assert_send<T: Send>(_t: T) {}
        assert_send(fut);
    }

    // ── SalvoRouter hoop/push convenience methods ────────────────

    #[test]
    fn salvo_router_hoop_and_push_compile() {
        let r = SalvoRouter::new();
        let _r = r.push(SalvoRouter::new());
    }
}
