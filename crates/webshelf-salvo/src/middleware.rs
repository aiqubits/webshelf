//! Salvo 生态中间件构建器。
//!
//! 提供与 `webshelf-axum`（tower-http）能力等价的中间件工厂函数。
//! 提供对称的认证、鉴权、限流中间件。
//! 服务端通过 `webshelf_salvo::middleware::*` 使用，不直接依赖 `salvo` crate。
//!
//! # CORS 注意事项
//!
//! salvo 的 CORS 中间件推荐应用在 Service 级别而非 Router 级别，
//! 因为浏览器 OPTIONS preflight 请求可能不匹配任何路由。
//! 当前实现将其应用在顶级 Router，对于已知路由能正常处理 CORS。
//! 若出现 CORS preflight 问题，需要将 CORS 配置传递到 `serve()` 方法。

use std::marker::PhantomData;

use async_trait::async_trait;
use salvo::http::Method;
use salvo::http::StatusCode;
use salvo::{Depot, FlowCtrl, Handler, Request, Response};

use crate::handler::CachedBody;
use webshelf_runtime::RateLimitGuard;
use webshelf_runtime::auth::{AuthUser, validate_jwt};
use webshelf_runtime::middleware::MiddlewareState;

/// CORS 配置，与 axum 的 CorsLayer 语义等价
///
/// # 行为等价性
/// - development 模式 + 空 origins → `AllowOrigin::Any`（Axum 的 `Any`）
/// - 非 development 模式 + 空 origins → 不设置 `allow_origin`，Cors 默认拒绝所有跨域请求
/// - 有效 origins → `AllowOrigin::list` 列表（Axum 的 `allow_origin(origins)`）
/// - 所有 origin 解析失败 + development → 回退到 `Any`
/// - 所有 origin 解析失败 + 非 development → 拒绝所有
///
/// ⚠️ CORS 检查由浏览器在客户端执行。当测试不加 `Origin` header 时，
/// Salvo 和 Axum 的 CORS 处理方式相同：不会拒绝请求（浏览器端 CORS 逻辑
/// 依赖于 response 中的 `Access-Control-Allow-Origin` 头，缺失该头时
/// 浏览器会阻止 JS 读取响应——但 HTTP 请求本身成功返回）。
pub struct CorsConfig {
    /// 允许的 Origin 列表
    pub allowed_origins: Vec<String>,
    /// 是否允许任意 origin（`*`）
    pub allow_any_origin: bool,
}

impl CorsConfig {
    /// 从 allowed_origins 列表和 env 构建配置
    pub fn from_origins(origins: &[String], env: &str) -> Self {
        if origins.is_empty() {
            if env == "development" {
                tracing::warn!(
                    "CORS: using Any (allow all origins). \
                     This is acceptable for development but NOT recommended for production."
                );
                return Self {
                    allowed_origins: vec![],
                    allow_any_origin: true,
                };
            }
            tracing::warn!(
                "CORS: no allowed_origins configured in {} environment. \
                 If using a reverse proxy (nginx) for same-origin serving, this is expected.",
                env
            );
            return Self {
                allowed_origins: vec![],
                allow_any_origin: false,
            };
        }

        let valid: Vec<String> = origins
            .iter()
            .filter(|o| *o == "*" || o.starts_with("http://") || o.starts_with("https://"))
            .cloned()
            .collect();

        if valid.is_empty() && env == "development" {
            tracing::warn!(
                "CORS: all configured allowed_origins failed to parse, falling back to Any"
            );
            return Self {
                allowed_origins: vec![],
                allow_any_origin: true,
            };
        }

        tracing::info!("CORS: allowing origins: {:?}", valid);
        Self {
            allowed_origins: valid,
            allow_any_origin: false,
        }
    }

    /// 构建 CORS handler（应用在 Router 级别）
    ///
    /// 注意：`into_handler()` 会消费 `self`，每次调用都会生成一个新的 handler。
    /// 支持多个 origin 的正确传递（使用 `AllowOrigin::list` 而非循环覆盖）。
    pub fn into_handler(self) -> impl salvo::Handler {
        let methods: &[Method] = &[
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ];

        if self.allow_any_origin {
            salvo::cors::Cors::new()
                .allow_origin(salvo::cors::Any)
                .allow_methods(methods.to_vec())
                .allow_headers("*")
                .into_handler()
        } else if self.allowed_origins.is_empty() {
            // 无 origin 配置时，不设置 allow_origin，Cors 默认拒绝所有跨域请求。
            // 与 axum 对齐：只允许 OPTIONS preflight，不允许其他方法。
            salvo::cors::Cors::new()
                .allow_methods(vec![Method::OPTIONS])
                .allow_headers("*")
                .into_handler()
        } else {
            // 通过 &Vec<String> 触发 From<&Vec<String>> for AllowOrigin，
            // 内部调用 AllowOrigin::list()，一次传递所有 origin。
            salvo::cors::Cors::new()
                .allow_origin(&self.allowed_origins)
                .allow_methods(methods.to_vec())
                .allow_headers("*")
                .into_handler()
        }
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec![],
            allow_any_origin: true,
        }
    }
}

/// 创建压缩中间件 handler
pub fn compression() -> impl salvo::Handler {
    salvo::compression::Compression::new()
}

/// 创建请求日志中间件 handler
pub fn logger() -> impl salvo::Handler {
    salvo::logging::Logger::new()
}

/// 创建请求体大小限制中间件 handler
pub fn max_body_size(max_bytes: u64) -> impl salvo::Handler {
    salvo::size_limiter::max_size(max_bytes)
}

/// 创建 panic 捕获中间件 handler
pub fn catch_panic() -> impl salvo::Handler {
    salvo::catch_panic::CatchPanic::new()
}

// ── 认证中间件 ─────────────────────────────────────

/// 认证中间件 —— 验证 JWT token 的签名/过期/issuer/audience，
/// 并检查 token_version 是否与数据库中当前版本匹配（实现 logout-all 功能）。
///
/// 与 axum 版本的 `auth_middleware` 对称：使用 `MiddlewareState` 抽象
/// 获取 jwt_secret 和 token_version 校验，业务逻辑委托给 `webshelf_runtime`。
///
/// 泛型参数 `S` 为共享状态类型（必须实现 `MiddlewareState` trait）。
pub struct AuthMiddleware<S>(PhantomData<S>);

impl<S> Default for AuthMiddleware<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> AuthMiddleware<S> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

#[async_trait]
impl<S> Handler for AuthMiddleware<S>
where
    S: MiddlewareState + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let path = req.uri().path();
        if path == "/health" || path == "/api/health" {
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let state: S = match depot.obtain::<S>() {
            Ok(s) => s.clone(),
            Err(_) => {
                tracing::error!("AuthMiddleware: state not found in Depot");
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR)
                    .render(salvo::writing::Json(serde_json::json!({"error": "internal_error", "message": "An unexpected error occurred"})));
                return;
            }
        };

        let token = match extract_bearer_token(req).or_else(|| extract_jwt_cookie(req)) {
            Some(t) => t,
            None => {
                res.status_code(StatusCode::UNAUTHORIZED)
                    .render(salvo::writing::Json(serde_json::json!({"error": "unauthorized", "message": "Missing or invalid Authorization header"})));
                return;
            }
        };

        match validate_jwt(&token, state.jwt_secret()) {
            Ok(claims) => {
                let user_id: i64 = match claims.sub.parse() {
                    Ok(id) => id,
                    Err(_) => {
                        tracing::warn!("Invalid user ID format in token: {}", claims.sub);
                        res.status_code(StatusCode::UNAUTHORIZED)
                            .render(salvo::writing::Json(serde_json::json!({"error": "unauthorized", "message": "Invalid or expired token"})));
                        return;
                    }
                };

                match state
                    .check_token_version(user_id, claims.token_version)
                    .await
                {
                    Ok(()) => {
                        depot.inject(AuthUser::from(claims));
                        ctrl.call_next(req, depot, res).await;
                    }
                    Err(e) => {
                        tracing::warn!("Token version validation failed: {}", e);
                        res.status_code(StatusCode::UNAUTHORIZED)
                            .render(salvo::writing::Json(serde_json::json!({"error": "unauthorized", "message": "Invalid or expired token"})));
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Token validation failed: {}", e);
                res.status_code(StatusCode::UNAUTHORIZED)
                    .render(salvo::writing::Json(serde_json::json!({"error": "unauthorized", "message": "Invalid or expired token"})));
            }
        }
    }
}

impl<S> Clone for AuthMiddleware<S> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

// ── 管理员权限守卫 ─────────────────────────────────

/// Admin 角色守卫 —— 检查当前用户是否为 admin/system。
/// 必须位于 `AuthMiddleware` 之后（需要 Depot 中有 AuthUser）。
pub struct RequireAdmin;

#[async_trait]
impl Handler for RequireAdmin {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let auth_user = match depot.obtain::<AuthUser>() {
            Ok(u) => u.clone(),
            Err(_) => {
                res.status_code(StatusCode::UNAUTHORIZED)
                    .render(salvo::writing::Json(serde_json::json!({"error": "unauthorized", "message": "Authentication required"})));
                return;
            }
        };

        if auth_user.role != "admin" && auth_user.role != "system" {
            res.status_code(StatusCode::FORBIDDEN)
                .render(salvo::writing::Json(serde_json::json!({"error": "forbidden", "message": "Admin privileges required"})));
            return;
        }

        ctrl.call_next(req, depot, res).await;
    }
}

// ── 限流中间件 ─────────────────────────────────────

/// Apply rate-limit middleware to a route (salvo equivalent of axum's `with_rate_limit_layer`).
///
/// Server route builders can use this single function regardless of
/// framework, eliminating duplicated route registration code.
pub fn with_rate_limit_hoop(
    route: crate::SalvoRouter,
    guard: RateLimitGuard,
) -> crate::SalvoRouter {
    route.hoop(RateLimitMiddleware { guard })
}

/// 通用限流中间件 Handler（对称于 axum 版本的 `rate_limit_middleware`）。
pub struct RateLimitMiddleware {
    pub guard: RateLimitGuard,
}

#[async_trait]
impl Handler for RateLimitMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if !self.guard.limiter.is_available() {
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let ip = extract_client_ip(req.headers()).or_else(|| extract_peer_ip(req));
        if let Some(ip) = ip {
            let ip_key = format!("{}:ip:{}", self.guard.key_prefix, ip);
            match self
                .guard
                .limiter
                .check(
                    &ip_key,
                    self.guard.ip_max_requests,
                    self.guard.ip_window_seconds,
                )
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(
                        "Rate limit exceeded (IP) for {}: {}",
                        self.guard.key_prefix,
                        ip
                    );
                    res.status_code(StatusCode::TOO_MANY_REQUESTS)
                        .render(salvo::writing::Json(serde_json::json!({"error": "rate_limited", "message": "Too many requests. Please try again later."})));
                    return;
                }
                Err(e) => {
                    tracing::error!(
                        "Rate-limit Redis error (IP) for {}: {:?}",
                        self.guard.key_prefix,
                        e
                    );
                    if !self.guard.limiter.fail_open() {
                        res.status_code(StatusCode::INTERNAL_SERVER_ERROR)
                            .render(salvo::writing::Json(serde_json::json!({"error": "internal_error", "message": "An unexpected error occurred"})));
                        return;
                    }
                }
            }
        }

        if let Some(email_max) = self.guard.email_max_requests {
            let bytes = match req.payload().await {
                Ok(b) => b.clone(),
                Err(e) => {
                    tracing::error!(
                        "Failed to read body for rate limiting ({}): {:?}",
                        self.guard.key_prefix,
                        e
                    );
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR)
                        .render(salvo::writing::Json(serde_json::json!({"error": "internal_error", "message": "An unexpected error occurred"})));
                    return;
                }
            };

            // 将 body 存入 Depot，使下游 UnifiedHandler 能从中读取而非从
            // req.payload() 再读（后者已被此中间件消耗）。
            depot.inject(CachedBody(bytes.clone()));

            if let Some(email) = extract_email_from_body(&bytes) {
                let email_key = format!("{}:email:{}", self.guard.key_prefix, email);
                match self
                    .guard
                    .limiter
                    .check(&email_key, email_max, self.guard.email_window_seconds)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(
                            "Rate limit exceeded (email) for {}: {}",
                            self.guard.key_prefix,
                            email
                        );
                        res.status_code(StatusCode::TOO_MANY_REQUESTS)
                            .render(salvo::writing::Json(serde_json::json!({"error": "rate_limited", "message": "Too many requests. Please try again later."})));
                        return;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Rate-limit Redis error (email) for {}: {:?}",
                            self.guard.key_prefix,
                            e
                        );
                        if !self.guard.limiter.fail_open() {
                            res.status_code(StatusCode::INTERNAL_SERVER_ERROR)
                                .render(salvo::writing::Json(serde_json::json!({"error": "internal_error", "message": "An unexpected error occurred"})));
                            return;
                        }
                    }
                }
            }
        }

        ctrl.call_next(req, depot, res).await;
    }
}

// ── 辅助函数 ───────────────────────────────────────

const BEARER_PREFIX: &[u8] = b"bearer ";

fn extract_bearer_token(req: &Request) -> Option<String> {
    let auth_header = req.headers().get(http::header::AUTHORIZATION)?;
    let auth_value = auth_header.to_str().ok()?;
    if auth_value.len() <= BEARER_PREFIX.len() {
        return None;
    }
    if !auth_value.as_bytes()[..BEARER_PREFIX.len()].eq_ignore_ascii_case(BEARER_PREFIX) {
        return None;
    }
    Some(auth_value[BEARER_PREFIX.len()..].to_string())
}

fn extract_jwt_cookie(req: &Request) -> Option<String> {
    let cookie_header = req.headers().get(http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;
    cookie_str
        .split(';')
        .map(str::trim)
        .filter_map(|s| cookie::Cookie::parse(s).ok())
        .find(|c| c.name() == "webshelf_jwt")
        .map(|c| c.value().to_string())
}

fn extract_client_ip(headers: &salvo::http::HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-forwarded-for")
        && let Ok(value) = value.to_str()
        && let Some(ip) = value.split(',').next()
        && !ip.trim().is_empty()
    {
        return Some(ip.trim().to_string());
    }
    if let Some(value) = headers.get("x-real-ip")
        && let Ok(value) = value.to_str()
        && !value.trim().is_empty()
    {
        return Some(value.trim().to_string());
    }
    None
}

fn extract_peer_ip(req: &Request) -> Option<String> {
    match req.remote_addr() {
        salvo::conn::SocketAddr::IPv4(addr) => Some(addr.ip().to_string()),
        salvo::conn::SocketAddr::IPv6(addr) => Some(addr.ip().to_string()),
        _ => None,
    }
}

fn extract_email_from_body(bytes: &[u8]) -> Option<String> {
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(bytes)
        && let Some(email) = val.get("email")?.as_str()
    {
        return Some(email.to_lowercase());
    }
    serde_urlencoded::from_bytes::<std::collections::HashMap<String, String>>(bytes)
        .ok()?
        .remove("email")
        .map(|e| e.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_client_ip tests ─────────────────────────────────

    #[test]
    fn extract_client_ip_from_x_forwarded_for() {
        let mut headers = salvo::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.1".parse().unwrap());
        assert_eq!(extract_client_ip(&headers), Some("203.0.113.1".to_string()));
    }

    #[test]
    fn extract_client_ip_from_x_forwarded_for_chain() {
        let mut headers = salvo::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "198.51.100.1, 10.0.0.1, 192.168.1.1".parse().unwrap(),
        );
        // Should take the leftmost (client) IP
        assert_eq!(
            extract_client_ip(&headers),
            Some("198.51.100.1".to_string())
        );
    }

    #[test]
    fn extract_client_ip_from_x_real_ip() {
        let mut headers = salvo::http::HeaderMap::new();
        headers.insert("x-real-ip", "192.168.1.42".parse().unwrap());
        assert_eq!(
            extract_client_ip(&headers),
            Some("192.168.1.42".to_string())
        );
    }

    #[test]
    fn extract_client_ip_x_forwarded_for_takes_priority() {
        let mut headers = salvo::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "10.0.0.1".parse().unwrap());
        headers.insert("x-real-ip", "192.168.1.100".parse().unwrap());
        assert_eq!(extract_client_ip(&headers), Some("10.0.0.1".to_string()));
    }

    #[test]
    fn extract_client_ip_missing_headers_returns_none() {
        let headers = salvo::http::HeaderMap::new();
        assert!(extract_client_ip(&headers).is_none());
    }

    #[test]
    fn extract_client_ip_empty_x_forwarded_for_ignored() {
        let mut headers = salvo::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "".parse().unwrap());
        assert!(extract_client_ip(&headers).is_none());
    }

    // ── extract_email_from_body tests ───────────────────────────

    #[test]
    fn extract_email_from_json_body() {
        let body = b"{\"email\": \"user@example.com\"}";
        assert_eq!(
            extract_email_from_body(body),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_from_json_body_normalizes_case() {
        let body = b"{\"email\": \"User@Example.COM\"}";
        assert_eq!(
            extract_email_from_body(body),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_from_form_body() {
        let body = b"email=user@example.com&password=secret";
        assert_eq!(
            extract_email_from_body(body),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_from_form_body_normalizes_case() {
        let body = b"email=User@Example.COM&password=secret";
        assert_eq!(
            extract_email_from_body(body),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_no_email_field_returns_none() {
        let body = b"{\"name\": \"test\"}";
        assert!(extract_email_from_body(body).is_none());
    }

    #[test]
    fn extract_email_from_empty_body_returns_none() {
        let body = b"";
        assert!(extract_email_from_body(body).is_none());
    }

    #[test]
    fn extract_email_from_invalid_json_tries_form_fallback() {
        // Not valid JSON but valid form-encoded
        let body = b"email=fallback@example.com";
        assert_eq!(
            extract_email_from_body(body),
            Some("fallback@example.com".to_string())
        );
    }
}
