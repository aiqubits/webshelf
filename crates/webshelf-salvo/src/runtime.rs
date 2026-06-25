use salvo::Listener;
use salvo::conn::TcpListener;
use std::time::Duration;
use webshelf_runtime::Runtime;

use crate::SalvoRouter;

/// Salvo 运行时的泛型标记类型。S 为共享状态类型。
///
/// Salvo 的 Router 没有状态类型参数（与 axum 不同），
/// 共享状态通过 `affix_state::inject` 中间件注入到 Depot 中。
/// 适配器在 `serve()` 方法内部注入状态。
#[derive(Clone, Copy)]
pub struct SalvoRuntime<S>(std::marker::PhantomData<S>);

impl<S: Clone + Send + Sync + 'static> Runtime for SalvoRuntime<S> {
    type Router = SalvoRouter;
    type MethodRouter = SalvoRouter;
    type State = S;

    fn new_router() -> Self::Router {
        SalvoRouter::new()
    }

    fn nest(router: Self::Router, path: &str, sub: Self::Router) -> Self::Router {
        let sub_inner = sub.into_inner();
        SalvoRouter::from_inner(
            router
                .into_inner()
                .push(salvo::Router::with_path(path).push(sub_inner)),
        )
    }

    fn merge(router: Self::Router, other: Self::Router) -> Self::Router {
        let other_inner = other.into_inner();
        SalvoRouter::from_inner(router.into_inner().push(other_inner))
    }

    fn with_route(router: Self::Router, path: &str, method: Self::MethodRouter) -> Self::Router {
        let method_inner = method.into_inner();
        let sub = salvo::Router::with_path(path).push(method_inner);
        SalvoRouter::from_inner(router.into_inner().push(sub))
    }

    fn with_state(router: Self::Router, _state: Self::State) -> Self::Router {
        // ⚠️ 有意为空操作（no-op）—— Salvo Router 没有类型状态参数，
        // 共享状态通过 affix_state::inject 在 serve() 中以中间件形式注入 Depot。
        // 参见 struct doc 和 serve() 实现。
        router
    }

    fn serve(
        router: Self::Router,
        state: Self::State,
        addr: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        let addr = addr.to_string();
        async move {
            let inner_router = router.into_inner();

            // 在顶级 Router 上挂载 affix_state 中间件注入共享状态，
            // 然后将用户构建的路由树作为子路由加入。
            let router = salvo::Router::new()
                .hoop(salvo::affix_state::inject(state))
                .push(inner_router);

            let acceptor = TcpListener::new(addr.clone()).bind().await;

            tracing::info!("Server is ready to accept connections on {}", addr);

            let server = salvo::Server::new(acceptor);
            let signal = server.handle();

            // Spawn shutdown watcher — when signal arrives, stop the server gracefully.
            tokio::spawn(async move {
                webshelf_runtime::shutdown_signal().await;
                tracing::info!("Received shutdown signal, stopping server gracefully");
                signal.stop_graceful(Duration::from_secs(10));
            });

            server.serve(router).await;

            tracing::info!("Server stopped");
            Ok(())
        }
    }
}

// Default impl 的 trait bound 与 Runtime trait 保持一致
impl<S: Clone + Send + Sync + 'static> Default for SalvoRuntime<S> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}
