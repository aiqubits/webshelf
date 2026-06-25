pub mod auth;
mod error;
pub mod middleware;
pub mod rate_limit;
mod request;
mod response;
mod runtime;
mod signal;

pub use auth::{AuthUser, JwtClaims, validate_jwt};
pub use error::HttpError;
pub use middleware::{MiddlewareState, validate_token};
pub use rate_limit::RateLimitGuard;
pub use request::RequestContext;
pub use response::{Response, ResponseBody};
pub use runtime::Runtime;
pub use signal::shutdown_signal;

#[cfg(test)]
mod tests {
    use crate::Runtime;

    #[derive(Clone)]
    struct MockState;

    #[derive(Clone, Copy)]
    struct MockRuntime;

    impl Runtime for MockRuntime {
        type Router = ();
        type MethodRouter = ();
        type State = MockState;

        fn new_router() -> Self::Router {}

        fn nest(router: Self::Router, _path: &str, _sub: Self::Router) -> Self::Router {
            router
        }

        fn merge(router: Self::Router, _other: Self::Router) -> Self::Router {
            router
        }

        fn with_route(
            router: Self::Router,
            _path: &str,
            _method: Self::MethodRouter,
        ) -> Self::Router {
            router
        }

        fn with_state(router: Self::Router, _state: Self::State) -> Self::Router {
            router
        }

        #[allow(clippy::manual_async_fn)]
        fn serve(
            _router: Self::Router,
            _state: Self::State,
            _addr: &str,
        ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
            async { Ok(()) }
        }
    }

    #[test]
    fn mock_runtime_compiles_and_works() {
        MockRuntime::new_router();
        let state = MockState;
        MockRuntime::merge((), ());
        MockRuntime::with_route((), "/test", ());
        MockRuntime::nest((), "/api", ());
        MockRuntime::with_state((), state);
    }

    #[test]
    fn mock_runtime_serve_returns_send_future() {
        let fut = MockRuntime::serve((), MockState, "0.0.0.0:0");
        fn assert_send<T: Send>(_t: T) {}
        assert_send(fut);
    }

    #[test]
    fn shutdown_signal_is_send_future() {
        use std::future::Future;
        fn assert_future<T: Future<Output = ()> + Send>(_f: T) {}
        assert_future(crate::shutdown_signal());
    }
}
