use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post, put},
};

use crate::AppState;
use crate::handlers::api::{
    create_user, delete_user, get_user, health_check, list_users, update_user,
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

    Router::new()
        .route("/health", get(health_check))
        .merge(admin_routes)
}
