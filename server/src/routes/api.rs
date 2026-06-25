use crate::AppRouter;
use crate::routes::helpers::{apply_admin_guard, delete, get, post, put};

use crate::handlers::api::{
    adjust_balance, change_my_password, create_user, delete_user, get_me, get_user, health_check,
    list_users, logout_all, set_balance, update_user,
};

pub fn api_routes() -> AppRouter {
    // Admin-only routes: require admin role
    let admin_routes = apply_admin_guard(
        AppRouter::new()
            .route("/users", get(list_users))
            .route("/users", post(create_user))
            .route("/users/{id}", get(get_user))
            .route("/users/{id}", put(update_user))
            .route("/users/{id}", delete(delete_user))
            .route("/users/{id}/balance", put(set_balance))
            .route("/users/{id}/balance/adjust", post(adjust_balance)),
    );

    // Self-service routes for any authenticated user (no admin role required).
    // Registered before admin_routes so /users/me matches before /users/{id}.
    let self_routes = AppRouter::new()
        .route("/users/me", get(get_me))
        .route("/users/me/password", post(change_my_password))
        .route("/users/me/logout-all", post(logout_all));

    AppRouter::new()
        .route("/health", get(health_check))
        .merge(self_routes)
        .merge(admin_routes)
}
