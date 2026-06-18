use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post, put},
};

use crate::AppState;
use crate::handlers::api::{
    adjust_balance, change_my_password, create_user, delete_user, get_me, get_user, health_check,
    list_users, logout_all, set_balance, update_user,
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
        .route("/users/{id}/balance", put(set_balance))
        .route("/users/{id}/balance/adjust", post(adjust_balance))
        .route_layer(axum_middleware::from_fn(require_admin));

    // Self-service routes for any authenticated user (no admin role required).
    // Registered before admin_routes so /users/me matches before /users/{id}.
    let self_routes = Router::new()
        .route("/users/me", get(get_me))
        .route("/users/me/password", post(change_my_password))
        .route("/users/me/logout-all", post(logout_all));

    Router::new()
        .route("/health", get(health_check))
        .merge(self_routes)
        .merge(admin_routes)
}
