mod runtime;
mod signal;

pub use runtime::Runtime;
pub use signal::shutdown_signal;

#[cfg(test)]
mod tests {
    use crate::Runtime;

    /// Minimal mock state for compile-time Runtime trait verification.
    #[derive(Clone)]
    struct MockState;

    /// Mock runtime: verifies that the trait contract is implementable
    /// with simple unit types, covering all required methods including
    /// `serve` which returns `impl Future + Send`.
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
        // Basic method chaining — confirms all trait methods exist and compose
        MockRuntime::new_router();
        let state = MockState;
        MockRuntime::merge((), ());
        MockRuntime::with_route((), "/test", ());
        MockRuntime::nest((), "/api", ());
        MockRuntime::with_state((), state);
    }

    #[test]
    fn mock_runtime_serve_returns_send_future() {
        // Verify that serve() returns a Future that satisfies Send,
        // which is required for tokio::spawn and other concurrent contexts.
        let fut = MockRuntime::serve((), MockState, "0.0.0.0:0");
        fn assert_send<T: Send>(_t: T) {}
        assert_send(fut);
    }

    #[test]
    fn shutdown_signal_is_send_future() {
        use std::future::Future;
        // Verify shutdown_signal() is a valid Future<Output = ()>
        fn assert_future<T: Future<Output = ()> + Send>(_f: T) {}
        assert_future(crate::shutdown_signal());
    }
}
