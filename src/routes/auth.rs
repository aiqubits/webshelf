use axum::{Router, routing::post};

use crate::AppState;
use crate::handlers::auth::{login, register};

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
}
