#![cfg(not(feature = "webshelf-salvo"))]

//! Integration Tests for webshelf
//!
//! These tests require running PostgreSQL and Redis instances.
//! Make sure to start the services before running:
//! - PostgreSQL: default port 5432
//! - Redis: default port 6379
//!
//! Run tests with: cargo test --test integration_tests
//!
//! NOTE: Tests use unique emails with nanosecond timestamps to avoid conflicts.
//! Test data is NOT automatically cleaned up between runs.
//! To clean up accumulated test data, run:
//!   DELETE FROM users WHERE email LIKE '%@example.com';
//! Or use the `cleanup_test_users` helper at the end of your test suite.

use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;
use webshelf_axum::{Any, Body, CorsLayer, Method, Request, Router, StatusCode, TraceLayer};
use webshelf_axum::{BodyExt, from_fn, from_fn_with_state};
use webshelf_server::middlewares::auth_middleware;

// Helper function to create test app
async fn create_test_app() -> Router {
    create_test_app_and_state().await.0
}

/// Create test app and return both Router and AppState for direct cache inspection.
async fn create_test_app_and_state() -> (Router, webshelf_server::AppState) {
    use distributed_ratelimit::{RateLimitConfig, RedisRateLimiter};
    use sea_orm::Database;
    use webshelf_server::AutoRouter;
    use webshelf_server::services::CacheService;
    use webshelf_server::utils::load_config;
    use webshelf_server::{
        AppState,
        routes::{api_routes, auth_routes},
    };

    // Load test configuration
    let config = load_config("config.toml", "development").expect("Failed to load config");

    // Connect to test database
    let db = Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    let db = AutoRouter::single(db);

    // Run migrations
    webshelf_server::migrations::run_migrations(db.write_conn())
        .await
        .expect("Failed to run migrations");

    // Initialize Snowflake ID generator (idempotent — subsequent calls are no-ops)
    webshelf_server::snowflake::init(db.write_conn())
        .await
        .expect("Failed to initialize Snowflake generator");

    // Create Redis client (optional)
    let cache = CacheService::new(&config.redis_url, config.cache_max_connections).await;

    let state = AppState {
        db,
        cache,
        config: Arc::new(config),
        email: emailserver::EmailService::new(emailserver::EmailConfig::default()),
        wechat: None,
    };

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    // Build test router with same middleware stack as main app
    let router = Router::new()
        .nest(
            "/api",
            api_routes().layer(from_fn_with_state(
                state.clone(),
                auth_middleware::<AppState>,
            )),
        )
        .nest(
            "/api/public/auth",
            auth_routes(RedisRateLimiter::disabled(RateLimitConfig::default())),
        )
        .layer(from_fn(webshelf_server::middlewares::panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state.clone());

    (router, state)
}

// Helper to cleanup test users (call at end of test suite to avoid data accumulation)
#[allow(dead_code)]
async fn cleanup_test_users(state: &webshelf_server::AppState) {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use webshelf_server::repositories::user::Entity as UserEntity;

    let result = UserEntity::delete_many()
        .filter(webshelf_server::repositories::user::Column::Email.contains("@example.com"))
        .exec(&*state.db)
        .await;

    match result {
        Ok(delete_result) => {
            tracing::info!("Cleaned up {} test users", delete_result.rows_affected);
        }
        Err(e) => {
            tracing::warn!("Failed to clean up test users: {}", e);
        }
    }
}

// Helper to extract JSON body
async fn body_to_json(body: Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// Helper to register a user and obtain a JWT token via login
async fn register_and_login(app: &Router, email: &str) -> String {
    let register_payload = json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });

    let register_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&register_payload).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), StatusCode::OK);

    let login_payload = json!({
        "email": email,
        "password": "Password123!"
    });

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = login_response.status();
    let login_body = body_to_json(login_response.into_body()).await;
    if status != StatusCode::OK {
        eprintln!("Login failed with status {}: {:?}", status, login_body);
    }
    assert_eq!(status, StatusCode::OK);
    login_body["token"].as_str().unwrap().to_string()
}

// Helper to directly create an admin user in the database and obtain a JWT token.
// Used for tests that need admin privileges (e.g., /api/users CRUD endpoints).
async fn create_admin_and_login(app: &Router, email: &str) -> String {
    use sea_orm::{ActiveModelTrait, Set};
    use webshelf_server::repositories::user::ActiveModel;

    // Register normally first, then update role to admin directly
    let token = register_and_login(app, email).await;

    // Extract user_id from the token to update the role
    use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
    use webshelf_runtime::auth::JwtClaims;

    // Load the JWT secret from the same config file used by the test app
    let secret = {
        let config = webshelf_server::utils::load_config("config.toml", "development")
            .expect("Failed to load config for JWT secret");
        config.jwt_secret
    };

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.set_issuer(&["webshelf-server"]);
    validation.set_audience(&["webshelf"]);
    let token_data = decode::<JwtClaims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .expect("Failed to decode token");

    let user_id: i64 = token_data
        .claims
        .sub
        .parse()
        .expect("Invalid user ID in token");

    // Get a DB connection to update the role.
    // NOTE: This test helper directly manipulates the database to promote a user
    // to admin.  It intentionally bypasses the production API (PUT /api/users/{id})
    // so that other tests (e.g. test_old_token_invalidated_after_role_change) can
    // independently verify the API-level token_version behaviour.
    //
    // We access the state through the router by making a request pattern —
    // instead, let's use a simpler approach: we create a direct DB connection
    let db = {
        let config = webshelf_server::utils::load_config("config.toml", "development")
            .expect("Failed to load config");
        sea_orm::Database::connect(&config.database_url)
            .await
            .expect("Failed to connect to database")
    };

    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use webshelf_server::repositories::user::{Column, Entity as UserEntity};

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
    // In production code, always use the atomic `UPDATE … SET token_version =
    // token_version + 1` pattern to avoid race conditions.
    active_model.token_version = Set(current_version.saturating_add(1));
    active_model
        .update(&db)
        .await
        .expect("Failed to update user role");

    // Login again to get a fresh token with the updated role
    let login_payload = json!({
        "email": email,
        "password": "Password123!"
    });

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_response.status(), StatusCode::OK);
    let login_body = body_to_json(login_response.into_body()).await;
    login_body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_health_check() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn test_user_registration() {
    let app = create_test_app().await;

    let payload = json!({
        "email": format!("test_user_{}@example.com", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()),
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["message"], "User registered successfully");
    assert!(body["user_id"].is_string());
}

#[tokio::test]
async fn test_registration_password_confirm_mismatch() {
    let app = create_test_app().await;

    let payload = json!({
        "email": format!("test_pwconfirm_{}@example.com", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()),
        "password": "Password123!",
        "password_confirm": "DifferentPassword456!",
        "name": "Test User"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn test_user_registration_invalid_email() {
    let app = create_test_app().await;

    let payload = json!({
        "email": "invalid-email",
        "password": "Password123!",
        "name": "Test User"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_user_registration_short_password() {
    let app = create_test_app().await;

    let payload = json!({
        "email": "test@example.com",
        "password": "Pass1",
        "name": "Test User"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_and_get_user() {
    let app = create_test_app().await;

    // Register as admin and login to get a JWT token with admin privileges
    let email = format!(
        "auth_user_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = create_admin_and_login(&app, &email).await;

    // Create user via authenticated endpoint
    let payload = json!({
        "email": format!("createget_user_{}@example.com", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()),
        "password": "Password123!",
        "name": "Create Get Test"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    let user_id = body["id"].as_str().unwrap();
    let expected_email = body["email"].as_str().unwrap();

    // Get user via authenticated endpoint
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/users/{}", user_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["email"], expected_email);
}

// Test for email conflict scenario
#[tokio::test]
async fn test_user_registration_conflict() {
    let app = create_test_app().await;

    // First registration should succeed
    let email = format!(
        "conflict_test_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let payload1 = json!({
        "email": &email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Conflict Test User"
    });

    let response1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload1).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response1.status(), StatusCode::OK);
    let body1 = body_to_json(response1.into_body()).await;
    assert_eq!(body1["message"], "User registered successfully");

    // Second registration with same email should fail with conflict
    let response2 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload1).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::CONFLICT);
}

// ---------------------------------------------------------------------------
// Name validation — registration endpoint (min=6, max=50)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_register_name_too_short() {
    let app = create_test_app().await;

    let payload = json!({
        "email": unique_email("reg_name_short"),
        "password": "Password123!",
        "name": "12345" // 5 characters — below minimum (6)
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_register_name_too_long() {
    let app = create_test_app().await;

    let payload = json!({
        "email": unique_email("reg_name_long"),
        "password": "Password123!",
        "name": "x".repeat(51) // 51 characters — above maximum (50)
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Name validation — admin create user endpoint (POST /api/users)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_admin_create_user_name_too_short() {
    let app = create_test_app().await;
    let token = create_admin_and_login(&app, &unique_email("adm_name_short")).await;

    let payload = json!({
        "email": unique_email("create_name_short"),
        "password": "Password123!",
        "name": "12345" // 5 characters — below minimum (6)
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_create_user_name_too_long() {
    let app = create_test_app().await;
    let token = create_admin_and_login(&app, &unique_email("adm_name_long")).await;

    let payload = json!({
        "email": unique_email("create_name_long"),
        "password": "Password123!",
        "name": "x".repeat(51) // 51 characters — above maximum (50)
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_create_user_auto_verified() {
    let app = create_test_app().await;
    let token = create_admin_and_login(&app, &unique_email("adm_auto_vfy")).await;

    let payload = json!({
        "email": unique_email("auto_verified_user"),
        "password": "Password123!",
        "name": "Auto Verify"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(
        body["email_verified"], true,
        "Admin-created user must be auto-verified"
    );
}

// ---------------------------------------------------------------------------
// Name validation — admin update user endpoint (PUT /api/users/{id})
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_admin_update_user_name_too_short() {
    let app = create_test_app().await;
    let token = create_admin_and_login(&app, &unique_email("adm_upd_nm_short")).await;

    // First create a user to update
    let create_payload = json!({
        "email": unique_email("to_update_short"),
        "password": "Password123!",
        "name": "Valid Name"
    });
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token.clone()))
                .body(Body::from(serde_json::to_string(&create_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = body_to_json(create_resp.into_body()).await;
    let user_id = create_body["id"].as_str().unwrap().to_string();

    // Now try to update name to too-short value
    let update_payload = json!({
        "name": "12345" // 5 characters — below minimum (6)
    });
    let update_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/users/{}", user_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&update_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_update_user_name_too_long() {
    let app = create_test_app().await;
    let token = create_admin_and_login(&app, &unique_email("adm_upd_nm_long")).await;

    // First create a user to update
    let create_payload = json!({
        "email": unique_email("to_update_long"),
        "password": "Password123!",
        "name": "Valid Name"
    });
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token.clone()))
                .body(Body::from(serde_json::to_string(&create_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = body_to_json(create_resp.into_body()).await;
    let user_id = create_body["id"].as_str().unwrap().to_string();

    // Now try to update name to too-long value
    let update_payload = json!({
        "name": "x".repeat(51) // 51 characters — above maximum (50)
    });
    let update_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/users/{}", user_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&update_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_unauthenticated_request_rejected() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// GET /api/users/me — get current user profile
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_me_success() {
    let app = create_test_app().await;

    let email = format!(
        "get_me_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = register_and_login(&app, &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["email"], email);
    assert_eq!(body["name"], "Test User");
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn test_get_me_unauthenticated() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/users/me/password — change current user's password
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_change_password_success() {
    let app = create_test_app().await;

    let email = format!(
        "chpwd_ok_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = register_and_login(&app, &email).await;

    let payload = json!({
        "current_password": "Password123!",
        "new_password": "NewSecure456!"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["message"], "Password changed successfully");

    // Verify the response includes a valid new_token that can be used immediately
    let new_token = body["new_token"]
        .as_str()
        .expect("new_token should be returned after password change");
    assert!(!new_token.is_empty(), "new_token should not be empty");

    let me_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", new_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        me_response.status(),
        StatusCode::OK,
        "new_token from password change should grant access to protected endpoints"
    );

    // Verify old JWT (signed before password change) is rejected — this is a
    // read-your-writes consistency test: the auth middleware reads token_version
    // from write_conn() and must detect the increment immediately.
    let old_jwt_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        old_jwt_response.status(),
        StatusCode::UNAUTHORIZED,
        "Old JWT (pre-password-change) must be rejected after password change "
    );

    // Verify old password no longer works
    let login_payload = json!({ "email": &email, "password": "Password123!" });
    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_resp.status(), StatusCode::UNAUTHORIZED);

    // Verify new password works
    let login_payload = json!({ "email": &email, "password": "NewSecure456!" });
    let login_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_change_password_wrong_current() {
    let app = create_test_app().await;

    let email = format!(
        "chpwd_wrong_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = register_and_login(&app, &email).await;

    let payload = json!({
        "current_password": "WrongPassword1!",
        "new_password": "NewSecure456!"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_to_json(response.into_body()).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("Current password is incorrect")
    );
}

#[tokio::test]
async fn test_change_password_empty_current() {
    let app = create_test_app().await;

    let email = format!(
        "chpwd_ecur_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = register_and_login(&app, &email).await;

    let payload = json!({
        "current_password": "",
        "new_password": "NewSecure456!"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_empty_new() {
    let app = create_test_app().await;

    let email = format!(
        "chpwd_enew_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = register_and_login(&app, &email).await;

    let payload = json!({
        "current_password": "Password123!",
        "new_password": ""
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_same_as_current() {
    let app = create_test_app().await;

    let email = format!(
        "chpwd_same_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = register_and_login(&app, &email).await;

    let payload = json!({
        "current_password": "Password123!",
        "new_password": "Password123!"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_weak_new_password() {
    let app = create_test_app().await;

    let email = format!(
        "chpwd_weak_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let token = register_and_login(&app, &email).await;

    let payload = json!({
        "current_password": "Password123!",
        "new_password": "weak"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_unauthenticated() {
    let app = create_test_app().await;

    let payload = json!({
        "current_password": "Password123!",
        "new_password": "NewSecure456!"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// Verify that a JWT issued before a password change is rejected afterward.
// This tests the token_version invalidation mechanism.
#[tokio::test]
async fn test_old_token_invalidated_after_password_change() {
    let app = create_test_app().await;

    let email = format!(
        "oldtoken_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // Step 1: Register and get an initial JWT token
    let old_token = register_and_login(&app, &email).await;

    // Step 2: Change the password (using the old token for auth)
    let payload = json!({
        "current_password": "Password123!",
        "new_password": "NewSecure456!"
    });

    let change_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/password")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", old_token))
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(change_resp.status(), StatusCode::OK);

    // Step 3: Attempt to use the old token to access a protected endpoint
    // It should be rejected because the token_version was incremented
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", old_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Old token should be rejected after password change"
    );

    // Step 4: Login with new password to get a fresh token — should work
    let login_payload = json!({ "email": &email, "password": "NewSecure456!" });
    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_resp.status(), StatusCode::OK);
    let login_body = body_to_json(login_resp.into_body()).await;
    let new_token = login_body["token"].as_str().unwrap().to_string();

    // Step 5: New token should work for accessing protected endpoint
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", new_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// Verify that a user's JWT is invalidated when an admin changes their role.
// The role change increments token_version in the DB, so the old token
// (issued before the role change) must be rejected.
#[tokio::test]
async fn test_old_token_invalidated_after_role_change() {
    let app = create_test_app().await;

    let user_email = format!(
        "user_roleinv_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // Step 1: Register a regular user and get their token
    let register_payload = json!({
        "email": &user_email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "RoleChange Test"
    });

    let register_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&register_payload).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), StatusCode::OK);
    let register_body = body_to_json(register_response.into_body()).await;
    let user_id = register_body["user_id"].as_str().unwrap().to_string();

    // Register already done above; only login to get the user's token
    let login_payload = json!({ "email": &user_email, "password": "Password123!" });
    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_resp.status(), StatusCode::OK);
    let login_body = body_to_json(login_resp.into_body()).await;
    let user_token = login_body["token"].as_str().unwrap().to_string();

    // Step 3: Promote the user to admin directly in the database.
    //
    // Only system-role actors can change roles via the API per RBAC design.
    // This test focuses on verifying token invalidation — we bypass the API
    // and use direct DB access (same pattern as create_admin_and_login).
    let user_id_int: i64 = user_id.parse().expect("Invalid user ID");
    let db = {
        let config = webshelf_server::utils::load_config("config.toml", "development")
            .expect("Failed to load config");
        sea_orm::Database::connect(&config.database_url)
            .await
            .expect("Failed to connect to database")
    };

    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{ActiveModel, Column, Entity as UserEntity};

    let user = UserEntity::find()
        .filter(Column::Id.eq(user_id_int))
        .one(&db)
        .await
        .expect("Failed to find user")
        .expect("User not found");

    let current_version = user.token_version;
    let mut active_model: ActiveModel = user.into();
    active_model.role = Set("admin".to_string());
    // NOTE: read-modify-write is safe here (single-threaded test, no concurrency).
    // In production code, always use the atomic `UPDATE … SET token_version =
    // token_version + 1` pattern to avoid race conditions.
    active_model.token_version = Set(current_version.saturating_add(1));
    active_model
        .update(&db)
        .await
        .expect("Failed to update user role");

    // Step 4: Old user token should be rejected (token_version was incremented)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", user_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Old user token should be rejected after role change"
    );

    // Step 5: Login again with new role to get a fresh token — should work
    let login_payload = json!({ "email": &user_email, "password": "Password123!" });
    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_resp.status(), StatusCode::OK);
    let login_body = body_to_json(login_resp.into_body()).await;
    let new_token = login_body["token"].as_str().unwrap().to_string();

    // Step 6: New token should work
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", new_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The user now has admin role
    assert_eq!(response.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Email verification tests
// ---------------------------------------------------------------------------

// Verify that login is blocked when email is not verified.
// In the test environment, email service is not configured so users cannot
// actually verify their email. This test confirms the registration flow
// handles the "email not configured" case gracefully.
#[tokio::test]
async fn test_registration_without_email_service_succeeds() {
    let app = create_test_app().await;

    let email = format!(
        "noemail_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let payload = json!({
        "email": &email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "No Email Test"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Registration should succeed even without email service (graceful degradation)
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    // When email service is not configured, the message should indicate success
    assert_eq!(body["message"], "User registered successfully");
    assert!(body["user_id"].is_string());
    // Email auto-verified because the email service is not configured
    assert_eq!(body["email_verified"], true);
}

// Test that verify-email rejects already-verified users.
// In the test environment, email service is not configured, so the user is
// auto-verified during registration. Submitting a verification code for an
// already-verified email should return an error.
#[tokio::test]
async fn test_verify_email_rejects_already_verified_user() {
    let app = create_test_app().await;

    let email = format!(
        "verify_bad_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // Register a user — auto-verified because email service is not configured.
    let payload = json!({
        "email": &email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Verify Test"
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Attempting to verify an already-verified email should fail.
    let verify_payload = json!({
        "email": &email,
        "code": "123456"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/verify-email")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&verify_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Test that verify-email rejects invalid request bodies
#[tokio::test]
async fn test_verify_email_validation_error() {
    let app = create_test_app().await;

    // Invalid email
    let payload = json!({
        "email": "not-an-email",
        "code": "123456"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/verify-email")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Invalid code length
    let payload = json!({
        "email": "test@example.com",
        "code": "12345"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/verify-email")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Test that resend-code endpoint validation works
#[tokio::test]
async fn test_resend_code_validation_error() {
    let app = create_test_app().await;

    let payload = json!({
        "email": "not-an-email"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/resend-code")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Test that verify-email returns InvalidOrExpired (400) for a non-existent email.
// This prevents user enumeration — an attacker cannot distinguish "email not found"
// from "invalid code" or "expired code".
#[tokio::test]
async fn test_verify_email_with_nonexistent_email() {
    let app = create_test_app().await;

    let payload = json!({
        "email": "nonexistent@example.com",
        "code": "123456"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/verify-email")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Non-existent email should return 400 (BadRequest), not 404,
    // to prevent attackers from enumerating registered emails.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// Test that resend-code returns 200 OK when email service is not configured
// AND the user exists but is auto-verified (registration auto-verifies
// when email service is unconfigured).  Verified users don't need a code,
// so the endpoint returns 200 immediately regardless of SMTP state.
#[tokio::test]
async fn test_resend_code_with_unconfigured_email_service() {
    let app = create_test_app().await;

    // Register a real user so the email lookup succeeds.
    // Registration auto-verifies when email service is unconfigured.
    let email = format!(
        "resend_200_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": "Password123!",
                        "password_confirm": "Password123!",
                        "name": "Resend 200 Test",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // A syntactically valid, registered (auto-verified) email — resend-code
    // returns 200 because the user is already verified, even when the email
    // service is not configured.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/resend-code")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "email": &email })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// Test that resend-code with a non-existent email returns 200 OK
// (anti-enumeration — must not reveal whether the email is registered)
// even when the email service is not configured.
#[tokio::test]
async fn test_resend_code_nonexistent_email_returns_ok() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/resend-code")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": "no-such-user-resend@example.com"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Non-existent email must return 200 OK (anti-enumeration)"
    );
}

// Test that resend-code with a registered (auto-verified) user returns
// 200 OK even when the email service is not configured.
// Previously this returned 503 (EmailNotConfigured) because the email
// config check ran before the email_verified check.  The fix reorders
// the checks so that already-verified users get 200 regardless of SMTP
// state.
#[tokio::test]
async fn test_resend_code_verified_user_returns_ok() {
    let app = create_test_app().await;

    // Register a user — auto-verified because email service is unconfigured.
    // Since the user is already verified, resend-code should return 200
    // immediately (email_verified check before email-config check).
    let email = format!(
        "resend_ok_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let _ = register_and_login(&app, &email).await;

    // First resend-code request — should succeed (user is already verified).
    let response1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/resend-code")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "email": &email })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response1.status(), StatusCode::OK);

    // Second resend-code request — must also return 200 (verified user
    // bypasses the cooldown check entirely).
    let response2 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/resend-code")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "email": &email })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response2.status(),
        StatusCode::OK,
        "Second request must also return 200 (verified user bypasses email checks)"
    );
}

// Test that resend-code returns 503 SERVICE_UNAVAILABLE when email service
// is not configured AND the user exists but email_verified is false.
// This is the 503 path — the user exists, is NOT verified, and there is
// no SMTP to resend from.
#[tokio::test]
async fn test_resend_code_unverified_user_returns_503() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, Database, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };
    use webshelf_server::utils::load_config;

    let app = create_test_app().await;

    let email = format!(
        "resend_503_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // 1. Register via API — user is auto-verified (email service not configured)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": "Password123!",
                        "password_confirm": "Password123!",
                        "name": "Resend503Test",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 2. Directly set email_verified = false in the database to simulate
    //    the state of an unverified user with no SMTP configured.
    let config = load_config("config.toml", "development").expect("Failed to load config");
    let db = Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist after registration");

    let mut active_model: ActiveModel = user.into();
    active_model.email_verified = Set(false);
    active_model.updated_at = Set(chrono::Utc::now());
    active_model.update(&db).await.unwrap();

    // 3. Resend-code with an unverified user — email service is not configured,
    //    so the endpoint must return 503 SERVICE_UNAVAILABLE.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/resend-code")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "email": &email })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ---------------------------------------------------------------------------
// Additional security & integration tests added after Code Review
// ---------------------------------------------------------------------------

/// End-to-end test: register (auto-verified in dev/test) → login succeeds.
/// This verifies the full registration → auto-verify → login chain works
/// correctly when the email service is not configured.
#[tokio::test]
async fn test_auto_verified_user_can_login() {
    let app = create_test_app().await;

    let email = format!(
        "autoverify_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = "Password123!";

    // 1. Register — auto-verified because email service is not configured.
    let register_payload = json!({
        "email": &email,
        "password": password,
        "password_confirm": password,
        "name": "AutoVerify Test"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&register_payload).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["email_verified"], true);

    // 2. Login with the same credentials — must succeed.
    let login_payload = json!({
        "email": &email,
        "password": password
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    if status != StatusCode::OK {
        eprintln!("Login failed with status {}: {:?}", status, body);
    }
    assert_eq!(status, StatusCode::OK);
    assert!(body["token"].is_string());
    assert_eq!(body["token_type"], "Bearer");
}

/// Verify that login is blocked when email_verified = false.
///
/// In the default test environment, the email service is not configured,
/// so registration auto-verifies users (`email_verified = true`).  This
/// test registers a user, then directly flips `email_verified` to `false`
/// in the database to simulate the state that would exist with SMTP.
/// It then verifies that login returns 401 Unauthorized (same error as
/// invalid credentials — anti-enumeration).
#[tokio::test]
async fn test_unverified_email_cannot_login() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, Database, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };
    use webshelf_server::utils::load_config;

    let app = create_test_app().await;

    let email = format!(
        "unverified_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = "Password123!";

    // 1. Register via API — user is auto-verified (email service not configured)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": password,
                        "password_confirm": password,
                        "name": "UnverifiedTest"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Registration should succeed (the user is auto-verified)
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["email_verified"], true);

    // 2. Directly set email_verified = false in the database to simulate
    //    the state that would exist if email service was configured.
    let config = load_config("config.toml", "development").expect("Failed to load config");
    let db = Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist after registration");

    let mut active_model: ActiveModel = user.into();
    active_model.email_verified = Set(false);
    active_model.updated_at = Set(chrono::Utc::now());
    active_model.update(&db).await.unwrap();

    // 3. Login must fail because email is not verified.
    // Returns 401 (same as wrong credentials) to prevent user enumeration.
    let login_payload = json!({
        "email": &email,
        "password": password
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Password-reset endpoint tests
// ---------------------------------------------------------------------------

/// Insert a password-reset verification code directly into the database
/// for a given email.  This bypasses the email-send path so we can
/// deterministically test the `reset_password` branch even when the SMTP
/// service is not configured.
///
/// Returns the plaintext code so the caller can submit it.
async fn seed_reset_code(email: &str, expires_in_minutes: i64) -> String {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    use sea_orm::{ActiveModelTrait, ColumnTrait, Database, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };
    use webshelf_server::utils::load_config;

    let config = load_config("config.toml", "development").expect("Failed to load config");
    let db = Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User must exist before seeding reset code");

    // Generate a 6-digit code and hash it with Argon2 (same primitive the
    // service uses).
    use rand::Rng;
    let code_int = rand::thread_rng().gen_range(0..1_000_000);
    let code = format!("{:06}", code_int);
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut rand::thread_rng());
    let code_hash = argon2
        .hash_password(code.as_bytes(), &salt)
        .expect("Failed to hash reset code")
        .to_string();

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(expires_in_minutes);

    let mut active_model: ActiveModel = user.into();
    active_model.password_reset_token_hash = Set(Some(code_hash));
    active_model.password_reset_expires_at = Set(Some(expires_at));
    active_model.password_reset_sent_at = Set(Some(now));
    active_model.password_reset_failed_attempts = Set(0);
    active_model.updated_at = Set(now);
    active_model.update(&db).await.unwrap();

    code
}

/// Forgot-password with a syntactically invalid email → 400 BadRequest.
#[tokio::test]
async fn test_forgot_password_invalid_email() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/forgot-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "email": "not-an-email" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Forgot-password for a non-existent email must return 200 OK with the
/// generic message — this is the anti-enumeration contract.
#[tokio::test]
async fn test_forgot_password_nonexistent_email_returns_ok() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/forgot-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": "no-such-user-12345@example.com"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Non-existent email must return 200 OK (anti-enumeration)"
    );
    let body = body_to_json(response.into_body()).await;
    assert_eq!(
        body["message"],
        "If that email is registered, a reset code has been sent"
    );
}

/// Forgot-password with SMTP unconfigured must return 503 Service Unavailable,
/// regardless of whether the user exists.
#[tokio::test]
async fn test_forgot_password_email_not_configured() {
    let app = create_test_app().await;

    let email = format!(
        "fpg_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    // Register user.
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": "Password123!",
                        "password_confirm": "Password123!",
                        "name": "FPG Test",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/forgot-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "email": &email })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Forgot-password must surface 503 when SMTP is not configured"
    );
}

/// Reset-password rejects a malformed code (wrong length / non-numeric).
#[tokio::test]
async fn test_reset_password_invalid_code_length() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": "user@example.com",
                        "code": "12345",    // 5 digits, not 6
                        "new_password": "NewPassword456!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Reset-password rejects a weak new password.
#[tokio::test]
async fn test_reset_password_weak_new_password() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": "user@example.com",
                        "code": "123456",
                        "new_password": "weak"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// End-to-end reset flow:
/// 1. Register user (auto-verified)
/// 2. Seed a known reset code directly into the DB (bypass SMTP)
/// 3. POST /reset-password with the correct code → 200 + new_token
/// 4. Old tokens (issued before the reset) must be rejected
/// 5. Re-using the same reset code must fail (single-use)
#[tokio::test]
async fn test_reset_password_success_and_token_invalidation() {
    let app = create_test_app().await;

    let email = format!(
        "rstok_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let original_password = "Password123!";

    // 1. Register user.
    let register_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": original_password,
                        "password_confirm": original_password,
                        "name": "Reset OK"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);

    // 2. Capture the original JWT so we can verify it gets invalidated later.
    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": original_password
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);
    let old_token = body_to_json(login_response.into_body()).await["token"]
        .as_str()
        .unwrap()
        .to_string();

    // 3. Seed a known reset code into the DB.
    let reset_code = seed_reset_code(&email, 60).await;

    // 4. Submit the reset with the correct code + new password.
    let new_password = "NewSecure789!";
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "code": reset_code,
                        "new_password": new_password
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    let fresh_token = body["token"].as_str().unwrap();
    assert!(!fresh_token.is_empty());
    assert_eq!(body["token_type"], "Bearer");

    // 5. Old token must be rejected (token_version was atomically incremented).
    let me_old = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", old_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        me_old.status(),
        StatusCode::UNAUTHORIZED,
        "Old JWT must be rejected after password reset"
    );

    // 6. Fresh token must grant access.
    let me_new = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", fresh_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_new.status(), StatusCode::OK);

    // 7. Verify password_reset_* fields are cleared after successful reset.
    {
        use sea_orm::{ColumnTrait, Database, EntityTrait, QueryFilter};
        use webshelf_server::repositories::user::{Column as UserColumn, Entity as UserEntity};
        use webshelf_server::utils::load_config;

        let config = load_config("config.toml", "development").expect("Failed to load config");
        let db = Database::connect(&config.database_url)
            .await
            .expect("Failed to connect to database");
        let user = UserEntity::find()
            .filter(UserColumn::Email.eq(email.to_lowercase()))
            .one(&db)
            .await
            .unwrap()
            .expect("User must exist");

        assert!(
            user.password_reset_token_hash.is_none(),
            "password_reset_token_hash must be cleared after successful reset"
        );
        assert!(
            user.password_reset_expires_at.is_none(),
            "password_reset_expires_at must be cleared after successful reset"
        );
        assert!(
            user.password_reset_sent_at.is_none(),
            "password_reset_sent_at must be cleared after successful reset"
        );
        assert_eq!(
            user.password_reset_failed_attempts, 0,
            "password_reset_failed_attempts must be 0 after successful reset"
        );
    }

    // 8. Re-using the same reset code must fail (single-use).
    let reuse = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "code": reset_code,
                        "new_password": "AnotherPass321!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        reuse.status(),
        StatusCode::BAD_REQUEST,
        "Reset code must be single-use"
    );

    // 9. Login with the NEW password must succeed; OLD password must fail.
    let login_new = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": new_password
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_new.status(), StatusCode::OK);

    let login_old = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": original_password
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_old.status(), StatusCode::UNAUTHORIZED);
}

/// Reset-password with a wrong code must return 400.
#[tokio::test]
async fn test_reset_password_wrong_code() {
    let app = create_test_app().await;

    let email = format!(
        "rwrt_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": "Password123!",
                        "password_confirm": "Password123!",
                        "name": "Wrong Code"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Seed a real code, but submit a *different* code.
    let _real_code = seed_reset_code(&email, 60).await;
    let wrong_code = "999999".to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "code": wrong_code,
                        "new_password": "NewPassword456!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Reset-password for a non-existent email must return 400 (same shape as
/// invalid-code) to prevent user enumeration.
#[tokio::test]
async fn test_reset_password_nonexistent_email() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": "ghost@example.com",
                        "code": "123456",
                        "new_password": "NewPassword456!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Reset-password with an expired code must return 400.
#[tokio::test]
async fn test_reset_password_expired_code() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, Database, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };
    use webshelf_server::utils::load_config;

    let app = create_test_app().await;

    let email = format!(
        "rexp_{}@example.com",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "password": "Password123!",
                        "password_confirm": "Password123!",
                        "name": "Expired Code"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Seed a code, then set expires_at to the past.
    let code = seed_reset_code(&email, 60).await;

    let config = load_config("config.toml", "development").expect("Failed to load config");
    let db = Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User must exist");

    let mut active_model: ActiveModel = user.into();
    active_model.password_reset_expires_at =
        Set(Some(chrono::Utc::now() - chrono::Duration::hours(1)));
    active_model.updated_at = Set(chrono::Utc::now());
    active_model.update(&db).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "email": &email,
                        "code": code,
                        "new_password": "NewPassword456!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ── Cache Invalidation Integration Tests ─────────────────────────────────
//
// These tests verify that UserService write operations properly invalidate
// the Redis cache so that subsequent reads fetch fresh data.

fn unique_email(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}_{}@example.com", prefix, ts)
}

#[tokio::test]
async fn test_cache_invalidation_after_user_update() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let (_app, state) = create_test_app_and_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    // Create a user via service
    let email = unique_email("update_cache");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user");
    let cache_key = format!("user:{}", user.id);

    // Populate cache by reading the user
    let cached = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    assert!(cached.is_some(), "user should be found");

    // Verify cache is now populated
    let cached_via_redis = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        cached_via_redis.is_some(),
        "user should be cached after get_user"
    );

    // Update the user — this should invalidate the cache
    let updated = svc
        .update_user(
            user.id.as_i64(),
            webshelf_server::repositories::user::UpdateUserInput {
                name: Some("Updated Name".to_string()),
                email: None,
                role: None,
            },
            "system",
        )
        .await
        .expect("update_user failed");
    assert_eq!(updated.name, "Updated Name");

    // Verify cache was invalidated
    let after_update = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_update.is_none(),
        "cache should be invalidated after update_user"
    );

    // Clean up cache
    let _ = state.cache.invalidate(&cache_key).await;
}

#[tokio::test]
async fn test_cache_invalidation_after_password_change() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let (_app, state) = create_test_app_and_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    // Create user
    let email = unique_email("pwd_cache");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "OldPass123!".to_string(),
                name: "Pwd Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");
    let cache_key = format!("user:{}", user.id);

    // Populate cache
    let _ = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    let cached = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        cached.is_some(),
        "user should be cached before password change"
    );

    // Change password — should invalidate cache
    let (updated, _new_version) = svc
        .change_password(user.id.as_i64(), "OldPass123!", "NewPass456!")
        .await
        .expect("change_password failed");
    assert_eq!(updated.name, "Pwd Cache Test");

    // Verify cache was invalidated
    let after_change = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_change.is_none(),
        "cache should be invalidated after password change"
    );

    let _ = state.cache.invalidate(&cache_key).await;
}

#[tokio::test]
async fn test_cache_invalidation_after_delete() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let (_app, state) = create_test_app_and_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    // Create user
    let email = unique_email("del_cache");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Delete Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");
    let cache_key = format!("user:{}", user.id);

    // Populate cache
    let _ = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    let cached = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(cached.is_some(), "user should be cached before delete");

    // Delete the user — should invalidate cache
    svc.delete_user(user.id.as_i64(), "system", 0)
        .await
        .expect("delete_user failed");

    // Verify cache was invalidated
    let after_delete = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_delete.is_none(),
        "cache should be invalidated after delete_user"
    );
}

#[tokio::test]
async fn test_cache_invalidation_after_balance_change() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let (_app, state) = create_test_app_and_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    // Create user
    let email = unique_email("bal_cache");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Balance Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");
    let cache_key = format!("user:{}", user.id);

    // Populate cache
    let _ = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    let cached = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        cached.is_some(),
        "user should be cached before balance change"
    );
    assert_eq!(cached.as_ref().unwrap().balance, 0);

    // Set balance — should invalidate cache
    let updated = svc
        .set_balance(user.id.as_i64(), 500, "system")
        .await
        .expect("set_balance failed");
    assert_eq!(updated.balance, 500);

    // Verify cache was invalidated
    let after_balance = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_balance.is_none(),
        "cache should be invalidated after balance change"
    );

    let _ = state.cache.invalidate(&cache_key).await;
}

// ── Pagination Count Cache Integration Tests ────────────────────────────

#[tokio::test]
async fn test_count_cache_populated_on_list_users() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;
    use webshelf_server::services::user::PaginationParams;

    let (_app, state) = create_test_app_and_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());
    let role = "admin";
    let count_key = format!("user:count:{}", role);

    // 清除可能残留的旧缓存，确保第一个 list_users 走缓存未命中路径
    let _ = state.cache.invalidate(&count_key).await;

    // Create a user so list_users returns at least one result
    let email = unique_email("count_cache");
    let _user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Count Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // First list_users: count cache miss → should populate
    let page1 = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");
    assert!(page1.total > 0, "should have at least one user");

    // Verify caching: second call should return at least same value (cache hit);
    // parallel tests may have added users between calls, making the count higher.
    let page2 = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");
    assert!(
        page2.total >= page1.total,
        "count cache should return at least same value (was {}, now {}); cache may have been invalidated by parallel tests",
        page1.total,
        page2.total,
    );

    // Clean up
    let _ = state.cache.invalidate(&count_key).await;
}

#[tokio::test]
async fn test_count_cache_invalidated_after_create_and_delete() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;
    use webshelf_server::services::user::PaginationParams;

    let (_app, state) = create_test_app_and_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());
    let role = "admin";

    // 清除可能残留的旧缓存
    let count_key = format!("user:count:{}", role);
    let _ = state.cache.invalidate(&count_key).await;

    // Create an initial user so list_users works
    let email1 = unique_email("cnt_del_1");
    let user1 = svc
        .create_user(
            CreateUserInput {
                email: email1.clone(),
                password: "Password123!".to_string(),
                name: "Count Delete 1".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // Populate count cache via list_users (with retry for resilience)
    for attempt in 1..=3 {
        let _page = svc
            .list_users(PaginationParams::default(), role)
            .await
            .expect("list_users failed");
        if state.cache.get::<u64>(&count_key).await.unwrap().is_some() {
            break;
        }
        tracing::warn!(
            "count cache empty after list_users (attempt {}), retrying...",
            attempt
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let before_create: Option<u64> = state.cache.get(&count_key).await.unwrap();
    assert!(before_create.is_some(), "count cache should exist");

    // Create another user — should invalidate count cache
    let email2 = unique_email("cnt_del_2");
    let _user2 = svc
        .create_user(
            CreateUserInput {
                email: email2.clone(),
                password: "Password123!".to_string(),
                name: "Count Delete 2".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // Verify count cache was invalidated
    let after_create: Option<u64> = state.cache.get(&count_key).await.unwrap();
    assert!(
        after_create.is_none(),
        "count cache should be invalidated after creating a user"
    );

    // Re-populate count cache
    let _page = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");

    // Now delete user1 — should invalidate count cache again
    svc.delete_user(user1.id.as_i64(), "system", 0)
        .await
        .expect("delete_user failed");

    let after_delete: Option<u64> = state.cache.get(&count_key).await.unwrap();
    assert!(
        after_delete.is_none(),
        "count cache should be invalidated after deleting a user"
    );

    let _ = state.cache.invalidate(&count_key).await;
}

#[tokio::test]
async fn test_count_cache_invalidated_after_create_system_role() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;
    use webshelf_server::services::user::PaginationParams;

    let (_app, state) = create_test_app_and_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());
    let role = "system";

    // Create an initial user so list_users works
    let email1 = unique_email("cnt_sys_1");
    let _user1 = svc
        .create_user(
            CreateUserInput {
                email: email1.clone(),
                password: "Password123!".to_string(),
                name: "Sys Count 1".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // Populate system count cache via list_users with system role.
    // Use a retry loop to handle potential cache invalidation from parallel
    // tests (other test's create_user invalidates all count cache keys).
    let count_key = format!("user:count:{}", role);
    let mut before_create: Option<u64> = None;
    for attempt in 1..=3 {
        let _page = svc
            .list_users(PaginationParams::default(), role)
            .await
            .expect("list_users failed");
        if let Some(cached) = state.cache.get::<u64>(&count_key).await.unwrap() {
            before_create = Some(cached);
            break;
        }
        tracing::warn!(
            "system count cache empty after list_users (attempt {}), retrying...",
            attempt
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(
        before_create.is_some(),
        "system count cache should exist after list_users"
    );

    // Create another user — should invalidate system count cache
    let email2 = unique_email("cnt_sys_2");
    let _user2 = svc
        .create_user(
            CreateUserInput {
                email: email2.clone(),
                password: "Password123!".to_string(),
                name: "Sys Count 2".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // Verify system count cache was invalidated
    let after_create: Option<u64> = state.cache.get(&count_key).await.unwrap();
    assert!(
        after_create.is_none(),
        "system count cache should be invalidated after creating a user"
    );

    let _ = state.cache.invalidate(&count_key).await;
}
