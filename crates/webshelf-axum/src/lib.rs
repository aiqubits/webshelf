mod route;
mod runtime;

pub use runtime::AxumRuntime;

// routing free functions（承载 axum 特有的 Handler 泛型约束）
pub use route::{delete, get, post, put};

// middleware free functions — 直接 re-export axum 原生函数，不二次封装
pub use axum::middleware::{from_fn, from_fn_with_state};

// ── Re-export axum 生态类型 ───────────────────────
pub use axum::body::{self, Body};
pub use axum::extract::connect_info::ConnectInfo;
pub use axum::extract::rejection; // 模块形式，允许 rejection::JsonRejection 等调用
pub use axum::extract::{Extension, FromRequest, Path, Query, Request, State};
pub use axum::http::header; // 模块形式，允许 header::SET_COOKIE 调用
pub use axum::http::{Extensions, HeaderMap, HeaderValue, Method, StatusCode};
pub use axum::middleware::{self, Next};
pub use axum::response::{IntoResponse, Response};
pub use axum::routing;
pub use axum::{Json, Router};

// tower-http 类型
pub use tower_http::compression::CompressionLayer;
pub use tower_http::cors::{Any, CorsLayer};
pub use tower_http::limit::RequestBodyLimitLayer;
pub use tower_http::trace::TraceLayer;

// 工具类型
pub use http_body_util::BodyExt;

#[cfg(test)]
mod tests {
    use crate::AxumRuntime;
    use webshelf_runtime::Runtime;

    /// Test state type for AxumRuntime verification.
    #[derive(Clone)]
    struct TestState;

    // ── Trait implementation check ──────────────────────────────

    #[test]
    fn axum_runtime_implements_runtime_trait() {
        fn assert_impl_runtime<T: Runtime>() {}
        assert_impl_runtime::<AxumRuntime<TestState>>();
    }

    // ── Re-export compilation checks ────────────────────────────

    /// Verify that all core types are re-exported and usable.
    /// Each `fn` is a compile-time assertion that the type exists
    /// in the crate's namespace.
    #[test]
    fn core_types_are_accessible() {
        // Router & JSON
        fn _r(_: crate::Router<()>) {}
        fn _j(_: crate::Json<()>) {}

        // Response types
        fn _ir(_: impl crate::IntoResponse) {}
        fn _res(_: crate::Response) {}

        // Extractor types
        fn _s(_: crate::State<TestState>) {}
        fn _p(_: crate::Path<String>) {}
        fn _q(_: crate::Query<()>) {}
        fn _e(_: crate::Extension<()>) {}
        fn _req(_: crate::Request) {}
        fn _fr<T: crate::FromRequest<()>>() {}

        // HTTP primitives
        fn _sc(_: crate::StatusCode) {}
        fn _hm(_: crate::HeaderMap) {}
        fn _hv(_: crate::HeaderValue) {}
        fn _m(_: crate::Method) {}
        fn _ext(_: crate::Extensions) {}

        // Modules — verify the re-exported module paths compile
        fn _hdr() {
            let _ = crate::header::SET_COOKIE;
        }
        fn _rj(_: &crate::rejection::JsonRejection) {}
    }

    /// Verify Body and body module are accessible.
    #[test]
    fn body_types_are_accessible() {
        let _ = crate::Body::empty();
        let _ = crate::body::Body::empty();
    }

    /// Verify middleware types compile.
    #[test]
    fn middleware_types_are_accessible() {
        fn _next(_: crate::Next) {}
        // from_fn with explicit state type annotation to help inference
        let _mw = crate::middleware::from_fn::<_, ()>(|| async {});
    }

    /// Verify routing free functions exist.
    #[test]
    fn routing_functions_are_accessible() {
        // Wrap in a Router<()> context so the state type S = () is inferred
        let router = crate::Router::<()>::new()
            .route("/hello", crate::get(|| async { "hello" }))
            .route("/created", crate::post(|| async { "created" }))
            .route("/updated", crate::put(|| async { "updated" }))
            .route("/deleted", crate::delete(|| async { "deleted" }));
        let _ = router;
    }

    /// Verify middleware free functions exist (from_fn / from_fn_with_state).
    ///
    /// This is a synchronous test — we only verify that the function signatures
    /// compile, not that the middleware actually runs (which would require a
    /// tokio runtime and a full Router setup).
    #[test]
    fn middleware_functions_are_accessible() {
        // from_fn with explicit state type annotation
        let _layer = crate::from_fn::<_, ()>(dummy_mw);
        let _layer_with_state = crate::from_fn_with_state::<_, _, ()>((), dummy_mw);
    }

    /// Async middleware handler — defined as a top-level function so the
    /// compiler resolves the async fn in trait without requiring tokio runtime.
    async fn dummy_mw(_req: crate::Request, next: crate::Next) -> crate::Response {
        next.run(_req).await
    }

    /// Verify tower-http types are re-exported.
    #[test]
    fn tower_http_types_are_accessible() {
        let _ = crate::CorsLayer::new();
        let _ = crate::CompressionLayer::new();
        let _ = crate::TraceLayer::new_for_http();
        let _ = crate::RequestBodyLimitLayer::new(1024);
        let _ = crate::Any;
    }

    /// Verify ConnectInfo is accessible.
    #[test]
    fn connect_info_is_accessible() {
        use std::net::SocketAddr;
        let _ = crate::ConnectInfo::<SocketAddr>;
    }

    // ── Router operation tests ──────────────────────────────────

    #[test]
    fn axum_runtime_router_operations() {
        type R = AxumRuntime<TestState>;

        let router = R::new_router();
        let merged = R::merge(router, R::new_router());
        let routed = R::with_route(merged, "/hello", crate::get(|| async { "world" }));
        let _nested = R::nest(routed, "/api", R::new_router());
    }

    #[test]
    fn axum_runtime_with_state_compiles() {
        type R = AxumRuntime<TestState>;

        let router = R::new_router();
        let state = TestState;
        let _with_state = R::with_state(router, state);
    }

    // ── Serve future Send check ─────────────────────────────────

    #[test]
    fn axum_runtime_serve_returns_send_future() {
        let fut = AxumRuntime::<TestState>::serve(crate::Router::new(), TestState, "0.0.0.0:0");
        fn assert_send<T: Send>(_t: T) {}
        assert_send(fut);
    }

    /// Verify BodyExt is accessible.
    #[test]
    fn body_ext_is_accessible() {
        fn _check<T: crate::BodyExt>() {}
    }
}
