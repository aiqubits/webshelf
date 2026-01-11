use axum::{routing::post, Router};

use crate::handlers::auth::{login, register};
use crate::AppState;

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
}
