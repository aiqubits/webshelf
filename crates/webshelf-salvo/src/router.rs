use std::sync::{Arc, Mutex};

/// 围绕 salvo::Router 的包装器，提供 Clone + Send + Sync 语义。
///
/// salvo::Router 自身不实现 Clone（因内部包含 `Vec<Box<dyn Filter>>`），
/// 因此通过 Arc<Mutex<>> 提供 Clone 能力。
///
/// **设计选择说明**：Mutex 在此处是一个纯设计选择（design choice），而非锁竞争方案。
/// 所有 Runtime 方法均消费型传递（consuming passing），同一 SalvoRouter 实例
/// 不会在路由构建期间被并发访问。Mutex 在消费型模式中实际上是零开销的——
/// 它仅服务于 Clone trait bound，不用于同步。
#[derive(Clone)]
pub struct SalvoRouter(Arc<Mutex<salvo::Router>>);

impl SalvoRouter {
    /// 创建空的包装 Router。
    /// 内部通过 Arc<Mutex<>> 满足 Clone 约束，
    /// 但在此消费型模式中不存在锁竞争。
    pub fn new() -> Self {
        SalvoRouter(Arc::new(Mutex::new(salvo::Router::new())))
    }

    /// 消费自己，取出内部 salvo::Router。
    /// 单次锁获取仅用于满足 take 语义，不涉及并发竞争。
    pub fn into_inner(self) -> salvo::Router {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::replace(&mut *guard, salvo::Router::new())
    }

    /// 从 salvo::Router 创建包装
    pub fn from_inner(inner: salvo::Router) -> Self {
        SalvoRouter(Arc::new(Mutex::new(inner)))
    }

    /// 应用转换：消费内部 Router，应用 f，返回新包装
    fn transform<F>(self, f: F) -> Self
    where
        F: FnOnce(salvo::Router) -> salvo::Router,
    {
        let inner = self.into_inner();
        SalvoRouter::from_inner(f(inner))
    }

    /// 在 router 上添加中间件（委托 salvo::Router::hoop）
    pub fn hoop<H: salvo::Handler>(self, handler: H) -> Self {
        self.transform(|r| r.hoop(handler))
    }

    /// 插入子路由（委托 salvo::Router::push）
    pub fn push(self, other: Self) -> Self {
        let other_inner = other.into_inner();
        self.transform(|r| r.push(other_inner))
    }

    /// 添加带路径和 handler 的路由（兼容 axum `.route()` 模式）
    pub fn route(self, path: &str, method: Self) -> Self {
        let method_inner = method.into_inner();
        let sub = salvo::Router::with_path(path).push(method_inner);
        self.transform(|r| r.push(sub))
    }

    /// 合并另一个路由（兼容 axum `.merge()` 模式）
    pub fn merge(self, other: Self) -> Self {
        let other_inner = other.into_inner();
        self.transform(|r| r.push(other_inner))
    }

    /// 在 path 前缀下嵌套子路由（兼容 axum `.nest()` 模式）
    pub fn nest(self, path: &str, sub: Self) -> Self {
        let sub_inner = sub.into_inner();
        let sub_with_path = salvo::Router::with_path(path).push(sub_inner);
        self.transform(|r| r.push(sub_with_path))
    }

    /// 添加中间件（委托 salvo::Router::hoop，兼容 axum `.layer()` 模式）
    pub fn layer<H: salvo::Handler>(self, handler: H) -> Self {
        self.hoop(handler)
    }

    /// 为当前路由添加 layer（委托 to layer，兼容 axum `.route_layer()` 模式）
    pub fn route_layer<H: salvo::Handler>(self, handler: H) -> Self {
        self.hoop(handler)
    }
}

impl Default for SalvoRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SalvoRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SalvoRouter").finish()
    }
}
