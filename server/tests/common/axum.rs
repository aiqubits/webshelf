//! Axum-specific test harness — creates test app and sends requests via tower::ServiceExt.

#![allow(dead_code)]

use distributed_ratelimit::RedisRateLimiter;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;
use webshelf_axum::{Body, BodyExt, Method, Request, Router, StatusCode};
use webshelf_server::AppState;

use crate::common;

/// Create a test router with the same middleware stack as the production app.
pub async fn create_app() -> Router {
    create_app_and_state().await.0
}

/// Create test router and return both Router and AppState for direct state inspection.
///
/// Uses the production `bootstrap::axum::build_app_router()` to build the
/// middleware chain. This ensures the test server stays in sync with the
/// production middleware chain. A disabled rate limiter is injected so that
/// tests do not hit per-IP rate limits (all requests originate from localhost).
pub async fn create_app_and_state() -> (Router, AppState) {
    let db = common::create_test_db_and_run_migrations().await;
    let cache = common::create_cache_service().await;
    let config = Arc::new(common::load_test_config());

    let state = AppState {
        db,
        cache,
        config,
        email: common::default_email_service(),
        wechat: None,
    };

    // 使用生产级的 build_app_router，注入禁用的 rate limiter
    let rate_limiter =
        RedisRateLimiter::disabled(distributed_ratelimit::RateLimitConfig::default());
    let router = webshelf_server::bootstrap::axum::build_app_router(
        state.clone(),
        "development",
        rate_limiter,
    );

    // Axum 的 build_app_router 不调用 .with_state()（与 Salvo 端对称），
    // 测试中通过 oneshot 发送请求前需要注入状态。
    let router = router.with_state(state.clone());

    (router, state)
}

/// Send an HTTP request through the axum router and return the response.
pub async fn send_request(
    app: &Router,
    method: Method,
    uri: &str,
    headers: Vec<(&str, &str)>,
    body: Body,
) -> webshelf_axum::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    for (key, value) in headers {
        builder = builder.header(key, value);
    }
    let request = builder.body(body).unwrap();
    app.clone().oneshot(request).await.unwrap()
}

/// Send a JSON body POST request through the axum router.
pub async fn send_json_post(app: &Router, uri: &str, body: &Value) -> webshelf_axum::Response {
    send_request(
        app,
        Method::POST,
        uri,
        vec![("content-type", "application/json")],
        Body::from(serde_json::to_string(body).unwrap()),
    )
    .await
}

/// Extract JSON body from axum Response.
pub async fn body_to_json(response: webshelf_axum::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Extract body bytes from axum Response.
pub async fn body_bytes(response: webshelf_axum::Response) -> bytes::Bytes {
    response.into_body().collect().await.unwrap().to_bytes()
}

/// Register a user and return JWT token.
pub async fn register_and_login(app: &Router, email: &str) -> String {
    let register_payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });

    let register_response =
        send_json_post(app, "/api/public/auth/register", &register_payload).await;
    assert_eq!(register_response.status(), StatusCode::OK);

    let login_payload = serde_json::json!({
        "email": email,
        "password": "Password123!"
    });

    let login_response = send_json_post(app, "/api/public/auth/login", &login_payload).await;
    assert_eq!(login_response.status(), StatusCode::OK);

    let login_body = body_to_json(login_response).await;
    login_body["token"].as_str().unwrap().to_string()
}

/// Register a user, then log in with `remember=true` and extract both the
/// JWT and the refresh-token cookie from the response. Returns `(jwt, refresh_token)`.
pub async fn register_and_login_with_refresh(app: &Router, email: &str) -> (String, String) {
    let register_payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });

    let resp = send_json_post(app, "/api/public/auth/register", &register_payload).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Login with remember=true to get JWT + refresh token
    let login_payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "remember": true,
    });

    let resp = send_json_post(app, "/api/public/auth/login", &login_payload).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Extract refresh token from Set-Cookie headers.
    let refresh_token = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .filter_map(|cookie_str| {
            if let Some(value_start) = cookie_str.find("webshelf_refresh") {
                let after_name = &cookie_str[value_start + "webshelf_refresh".len()..];
                if after_name.starts_with('=') {
                    let value_end = after_name.find(';').unwrap_or(after_name.len());
                    Some(after_name[1..value_end].to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .next()
        .expect("Set-Cookie for webshelf_refresh not found");

    // Parse the JSON body for the JWT.
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let jwt = body["token"].as_str().unwrap().to_string();

    (jwt, refresh_token)
}

/// Create an admin user in the database and return the JWT token.
pub async fn create_admin_and_login(app: &Router, email: &str) -> String {
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_runtime::auth::JwtClaims;
    use webshelf_server::repositories::user::{ActiveModel, Column, Entity as UserEntity};

    // Register normally first
    let token = register_and_login(app, email).await;

    // Decode token to get user_id
    let secret = common::load_test_config().jwt_secret;
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    validation.set_issuer(&["webshelf-server"]);
    validation.set_audience(&["webshelf"]);
    let token_data = jsonwebtoken::decode::<JwtClaims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .expect("Failed to decode token");

    let user_id: i64 = token_data
        .claims
        .sub
        .parse()
        .expect("Invalid user ID in token");

    // Connect to DB directly to update the user role to admin
    let config = common::load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let user = UserEntity::find()
        .filter(Column::Id.eq(user_id))
        .one(&db)
        .await
        .expect("Failed to find user")
        .expect("User not found");

    let current_version = user.token_version;
    let mut active_model: ActiveModel = user.into();
    active_model.role = Set("admin".to_string());
    // NOTE: read-modify-write is safe here (single-threaded test, no concurrency).
    // In production code, always use the atomic UPDATE … SET token_version = token_version + 1 pattern.
    active_model.token_version = Set(current_version.saturating_add(1));
    active_model.updated_at = Set(chrono::Utc::now());
    active_model
        .update(&db)
        .await
        .expect("Failed to update user to admin");

    // Re-login to get a new token with the updated role
    let login_payload = serde_json::json!({
        "email": email,
        "password": "Password123!"
    });
    let login_response = send_json_post(app, "/api/public/auth/login", &login_payload).await;
    assert_eq!(login_response.status(), StatusCode::OK);
    let login_body = body_to_json(login_response).await;
    login_body["token"].as_str().unwrap().to_string()
}
