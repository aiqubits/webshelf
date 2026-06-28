//! Salvo-specific bootstrap: router construction.

use crate::handlers::wechat::{wechat_callback_get, wechat_callback_post};
use crate::middlewares::AuthMiddleware;
use crate::routes::helpers::{get, post};
use crate::routes::{api_routes, auth_routes};
use crate::{AppRouter, AppState};
use distributed_ratelimit::RedisRateLimiter;
use webshelf_salvo::middleware::{CorsConfig, catch_panic, compression, logger, max_body_size};

/// Build application router — Salvo version
///
/// `rate_limiter` is accepted as a parameter so that test helpers can inject
/// a disabled limiter without duplicating the middleware chain.
/// Production code should call the convenience wrapper in `bootstrap::mod.rs`.
pub fn build_app_router(state: AppState, env: &str, rate_limiter: RedisRateLimiter) -> AppRouter {
    let allowed_origins = state.config.server.allowed_origins.clone();
    let cors_config = CorsConfig::from_origins(&allowed_origins, env);
    let cors_handler = cors_config.into_handler();

    // 与 axum 版本保持一致的中间件链顺序（从外到内）：
    //   max_body_size → compression → cors → logger → catch_panic
    //   → route matching → AuthMiddleware
    //
    // 注意: Salvo 的 hoop 按插入顺序执行（先添加 = 先处理请求 = 最外层），
    //       与 Axum 的 layer（后添加 = 最外层）相反。
    //       这里将外层中间件先添加，以匹配 Axum 的请求处理管道顺序。
    //       AuthMiddleware 通过 nest 内部的 hoop 只对 /api 路径生效，
    //       不影响 /api/public/auth 路径。
    AppRouter::new()
        .nest("/api", api_routes().hoop(AuthMiddleware::<AppState>::new()))
        .nest("/api/public/auth", auth_routes(rate_limiter))
        // Conditionally register WeChat callback routes.
        .merge(if state.wechat.is_some() {
            AppRouter::new()
                .route("/api/public/wechat/callback", get(wechat_callback_get))
                .route("/api/public/wechat/callback", post(wechat_callback_post))
        } else {
            AppRouter::new()
        })
        .hoop(max_body_size(10 * 1024 * 1024))
        .hoop(compression())
        .hoop(cors_handler)
        .hoop(logger())
        .hoop(catch_panic())
}
