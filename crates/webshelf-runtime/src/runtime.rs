/// Runtime trait — abstracts web framework infrastructure operations.
/// Each adapter (webshelf-axum / webshelf-salvo) implements this trait.
/// All methods are static (adapters like `AxumRuntime<S>` are ZSTs with no instance state).
pub trait Runtime: Clone + Send + Sync + Sized + 'static {
    /// 路由类型（axum::Router<S>, salvo::Router）
    type Router: Clone + Send + Sync;

    /// 方法路由类型（get/post/put/delete 的返回值）
    type MethodRouter: Clone + Send + Sync;

    /// 共享状态类型（由 server 的 bootstrap 注入，不在适配器中固定）
    type State: Clone + Send + Sync;

    fn new_router() -> Self::Router;

    /// 在 path 前缀下嵌套子路由
    fn nest(router: Self::Router, path: &str, sub: Self::Router) -> Self::Router;

    /// 合并另一个路由
    fn merge(router: Self::Router, other: Self::Router) -> Self::Router;

    /// 注册路径 + 方法路由
    fn with_route(router: Self::Router, path: &str, method: Self::MethodRouter) -> Self::Router;

    /// 注入共享状态。
    ///
    /// **语义差异说明**：
    /// - Axum 端：通过 `router.with_state(state)` 注入状态，路由可直接 serve。
    /// - Salvo 端：**空操作（no-op）**，因为 Salvo 的 Router 没有类型状态参数，
    ///   共享状态在 `serve()` 中通过 `affix_state::inject` 中间件注入 Depot。
    ///   因此 Salvo 模式下 `with_state()` 返回的路由在未经 `serve()` 处理前不可直接使用。
    fn with_state(router: Self::Router, state: Self::State) -> Self::Router;

    /// Bind address and start HTTP service with graceful shutdown.
    /// State is passed separately because adapters manage with_state internally.
    fn serve(
        router: Self::Router,
        state: Self::State,
        addr: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
