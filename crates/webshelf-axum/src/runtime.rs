use anyhow::Context;
use webshelf_runtime::Runtime;

/// Axum runtime marker type. S is the shared state type.
#[derive(Clone, Copy)]
pub struct AxumRuntime<S>(std::marker::PhantomData<S>);

impl<S: Clone + Send + Sync + 'static> Runtime for AxumRuntime<S> {
    type Router = axum::Router<S>;
    type MethodRouter = axum::routing::MethodRouter<S>;
    type State = S;

    fn new_router() -> Self::Router {
        axum::Router::new()
    }

    fn nest(router: Self::Router, path: &str, sub: Self::Router) -> Self::Router {
        router.nest(path, sub)
    }

    fn merge(router: Self::Router, other: Self::Router) -> Self::Router {
        router.merge(other)
    }

    fn with_route(router: Self::Router, path: &str, method: Self::MethodRouter) -> Self::Router {
        router.route(path, method)
    }

    fn with_state(router: Self::Router, state: Self::State) -> Self::Router {
        router.with_state(state)
    }

    fn serve(
        router: Self::Router,
        state: Self::State,
        addr: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        let addr = addr.to_string();
        async move {
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .with_context(|| format!("Failed to bind to address: {addr}"))?;
            tracing::info!("Server is ready to accept connections on {}", addr);
            let svc = router
                .with_state(state)
                .into_make_service_with_connect_info::<std::net::SocketAddr>();
            axum::serve(listener, svc)
                .with_graceful_shutdown(webshelf_runtime::shutdown_signal())
                .await
                .context("Server failed")?;
            Ok(())
        }
    }
}

impl<S: Clone + Send + Sync + 'static> Default for AxumRuntime<S> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}
