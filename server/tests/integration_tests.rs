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

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

// Helper function to create test app
async fn create_test_app() -> Router {
    use redis::Client as RedisClient;
    use sea_orm::Database;
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

    // Run migrations
    webshelf_server::migrations::run_migrations(&db)
        .await
        .expect("Failed to run migrations");

    // Create Redis client (optional)
    let redis = RedisClient::open(config.redis_url.as_str()).ok();

    let state = AppState {
        db,
        redis,
        config: Arc::new(config),
    };

    use http::Method;
    use tower_http::cors::CorsLayer;
    use tower_http::trace::TraceLayer;
    use webshelf_server::middlewares::auth::{JwtSecret, auth_middleware};

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any);

    // Build test router with same middleware stack as main app
    // Extension(JwtSecret) must be outermost (added last) so it injects
    // JwtSecret into extensions before auth_middleware reads it.
    Router::new()
        .nest("/api", api_routes())
        .nest("/api/public/auth", auth_routes())
        .layer(axum::middleware::from_fn(auth_middleware))
        .layer(axum::middleware::from_fn(
            webshelf_server::middlewares::panic::panic_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(axum::Extension(JwtSecret(state.config.jwt_secret.clone())))
        .with_state(state)
}

// Helper to cleanup test users (call at end of test suite to avoid data accumulation)
#[allow(dead_code)]
async fn cleanup_test_users(state: &webshelf_server::AppState) {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use webshelf_server::repositories::user::Entity as UserEntity;

    let result = UserEntity::delete_many()
        .filter(webshelf_server::repositories::user::Column::Email.contains("@example.com"))
        .exec(&state.db)
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

    assert_eq!(login_response.status(), StatusCode::OK);
    let login_body = body_to_json(login_response.into_body()).await;
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
    use webshelf_server::middlewares::auth::Claims;

    // Load the JWT secret from the same config file used by the test app
    let secret = {
        let config = webshelf_server::utils::load_config("config.toml", "development")
            .expect("Failed to load config for JWT secret");
        config.jwt_secret
    };

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .expect("Failed to decode token");

    let user_id = uuid::Uuid::parse_str(&token_data.claims.sub).expect("Invalid user ID");

    // Get a DB connection to update the role
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

    let mut active_model: ActiveModel = user.into();
    active_model.role = Set("admin".to_string());
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
