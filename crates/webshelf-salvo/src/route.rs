use crate::{SalvoRouter, UnifiedHandler, UnifiedRequest};
use std::future::Future;
use webshelf_runtime::{HttpError, Response};

/// 创建 GET 方法路由（接受统一 async handler，自动包装为 UnifiedHandler）
pub fn get<H, F>(handler: H) -> SalvoRouter
where
    H: Fn(UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send + 'static,
{
    SalvoRouter::from_inner(salvo::Router::new().get(UnifiedHandler(handler)))
}

/// 创建 POST 方法路由
pub fn post<H, F>(handler: H) -> SalvoRouter
where
    H: Fn(UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send + 'static,
{
    SalvoRouter::from_inner(salvo::Router::new().post(UnifiedHandler(handler)))
}

/// 创建 PUT 方法路由
pub fn put<H, F>(handler: H) -> SalvoRouter
where
    H: Fn(UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send + 'static,
{
    SalvoRouter::from_inner(salvo::Router::new().put(UnifiedHandler(handler)))
}

/// 创建 DELETE 方法路由
pub fn delete<H, F>(handler: H) -> SalvoRouter
where
    H: Fn(UnifiedRequest) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response, HttpError>> + Send + 'static,
{
    SalvoRouter::from_inner(salvo::Router::new().delete(UnifiedHandler(handler)))
}
