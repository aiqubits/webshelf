/// Runtime trait — 抽象 web 框架的基础设施操作。
///
/// 定义在独立的 `webshelf-runtime` crate 中，避免循环依赖。
/// 每个 web 框架适配器（webshelf-axum / webshelf-salvo）实现此 trait。
/// Handler/Middleware 的构造不在此 trait 中，而是作为 free function。
///
/// 当前所有方法均为**静态方法**（通过 `Self::method()` 调用），
/// 因为适配器类型（如 `AxumRuntime<S>`）是零大小类型（ZST），不持有实例状态。
/// 若未来某适配器需要实例内部状态，需重新评估设计。
pub trait Runtime: Clone + Send + Sync + Sized + 'static {
    /// 路由类型（axum::Router<S>, salvo::Router）
    type Router: Clone + Send + Sync;

    /// 方法路由类型（get/post/put/delete 的返回值）
    type MethodRouter: Clone + Send + Sync;

    /// 共享状态类型（由 server 的 bootstrap 注入，不在适配器中固定）
    type State: Clone + Send + Sync;

    // ── Router 构造 ───────────────────────────────

    /// 创建空 Router
    fn new_router() -> Self::Router;

    /// 在 path 前缀下嵌套子路由
    fn nest(router: Self::Router, path: &str, sub: Self::Router) -> Self::Router;

    /// 合并另一个路由
    fn merge(router: Self::Router, other: Self::Router) -> Self::Router;

    /// 注册路径 + 方法路由
    fn with_route(router: Self::Router, path: &str, method: Self::MethodRouter) -> Self::Router;

    /// 注入共享状态
    fn with_state(router: Self::Router, state: Self::State) -> Self::Router;

    // ── Server 启动 ───────────────────────────────

    /// 绑定地址并启动 HTTP 服务（内部创建 TcpListener + 处理 graceful shutdown）。
    /// 接收已构建好的 Router 和共享状态，内部调用框架的 with_state + serve。
    /// 之所以 state 单独传递而非在 Router 内注入，是因为 axum 0.8 要求
    /// Router<()> 才能调用 into_make_service()，适配器内部需先 with_state 再 serve。
    ///
    /// 使用显式 `impl Future + Send` 而非 `async fn` 来确保返回的 Future 满足 `Send`
    /// bound，这是 tokio::spawn 等并发上下文的必要条件。
    fn serve(
        router: Self::Router,
        state: Self::State,
        addr: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
