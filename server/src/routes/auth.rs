use axum::{Router, routing::post};

use crate::AppState;
use crate::handlers::auth::{login, register, resend_code, verify_email};

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/verify-email", post(verify_email))
        .route("/resend-code", post(resend_code))
}
