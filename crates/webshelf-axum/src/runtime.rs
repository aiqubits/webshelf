use anyhow::Context;
use webshelf_runtime::Runtime;

/// Axum 运行时的泛型标记类型。S 为共享状态类型。
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
        // 将 addr 转为 owned String 以便 async move 捕获
        let addr = addr.to_string();
        async move {
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .with_context(|| format!("Failed to bind to address: {addr}"))?;
            tracing::info!("Server is ready to accept connections on {}", addr);
            // with_state 注入 state 后 Router 变为 Router<()>，然后调用 into_make_service_with_connect_info
            // 注入 ConnectInfo<SocketAddr> 以便 rate-limit 中间件的 extract_peer_ip 能获取客户端 IP
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

// Default impl 的 trait bound 与 Runtime trait 保持一致
impl<S: Clone + Send + Sync + 'static> Default for AxumRuntime<S> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}
