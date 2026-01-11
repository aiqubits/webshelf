use axum::{routing::{delete, get, post, put}, Router};

use crate::handlers::api::{create_user, delete_user, get_user, health_check, list_users, update_user};
use crate::AppState;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/users", get(list_users))
        .route("/users", post(create_user))
        .route("/users/{id}", get(get_user))
        .route("/users/{id}", put(update_user))
        .route("/users/{id}", delete(delete_user))
}
