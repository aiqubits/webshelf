use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post, put},
};

use crate::AppState;
use crate::handlers::api::{
    change_my_password, create_user, delete_user, get_me, get_user, health_check, list_users,
    update_user,
};
use crate::middlewares::require_admin;

pub fn api_routes() -> Router<AppState> {
    // Admin-only routes: require admin role
    let admin_routes = Router::new()
        .route("/users", get(list_users))
        .route("/users", post(create_user))
        .route("/users/{id}", get(get_user))
        .route("/users/{id}", put(update_user))
        .route("/users/{id}", delete(delete_user))
        .route_layer(axum_middleware::from_fn(require_admin));

    // 任意已认证用户的自我管理路由（不需要 admin 角色）。
    // 放在 admin_routes 之前注册，确保 /users/me 优先于 /users/{id} 匹配。
    let self_routes = Router::new()
        .route("/users/me", get(get_me))
        .route("/users/me/password", post(change_my_password));

    Router::new()
        .route("/health", get(health_check))
        .merge(self_routes)
        .merge(admin_routes)
}
