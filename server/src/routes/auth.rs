use axum::{Router, routing::post};

use crate::AppState;
use crate::handlers::auth::{
    forgot_password, login, register, resend_code, reset_password, verify_email,
};

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/verify-email", post(verify_email))
        .route("/resend-code", post(resend_code))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
}
