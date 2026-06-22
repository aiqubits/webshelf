pub mod bootstrap;
pub mod handlers;
pub mod middlewares;
pub mod migrations;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod utils;
pub use utils::snowflake;

use std::sync::Arc;
pub use utils::AppConfig;
pub use utils::db_router::AutoRouter;

use crate::services::CacheService;

/// Application shared state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<AutoRouter>,
    /// Unified Redis-backed cache service (bb8 pool).
    /// Gracefully degrades to no-op when Redis is unavailable.
    pub cache: CacheService,
    pub config: Arc<AppConfig>,
    pub email: emailserver::EmailService,
}

// ── Web 框架抽象层 ──────────────────────────────────
// Runtime trait 来自 webshelf-runtime（独立 crate，无循环依赖）
pub use webshelf_runtime::Runtime;

// ── 条件编译：按 feature flag 选择框架适配器 ─────────
// 默认（没有任何 feature 时）走 webshelf-axum；
// 启用 webshelf-salvo 时走 salvo。
// 这样 cargo run --features webshelf-salvo 即可切换，无需 --no-default-features。
#[cfg(not(feature = "webshelf-salvo"))]
pub use webshelf_axum::*;

#[cfg(not(feature = "webshelf-salvo"))]
pub type AppRuntime = AxumRuntime<AppState>;

#[cfg(not(feature = "webshelf-salvo"))]
pub type AppRouter = <AppRuntime as Runtime>::Router;

// 待 salvo 适配完成后启用以下代码（需移除 #[cfg(...)] 上的注释）
// #[cfg(feature = "webshelf-salvo")]
// pub use webshelf_salvo::*;
// pub type AppRuntime = SalvoRuntime<AppState>;
// pub type AppRouter = <AppRuntime as Runtime>::Router;
