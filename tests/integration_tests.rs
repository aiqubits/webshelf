//! Integration Tests for webshelf
//! 
//! These tests require running PostgreSQL and Redis instances.
//! Make sure to start the services before running:
//! - PostgreSQL: default port 5432
//! - Redis: default port 6379
//! 
//! Run tests with: cargo test --test integration_tests

use axum::{body::Body, http::{Request, StatusCode}, Router};
use http_body_util::BodyExt;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

// Helper function to create test app
async fn create_test_app() -> Router {
    use webshelf::{AppState, routes::{api_routes, auth_routes}};
    use sea_orm::Database;
    use redis::Client as RedisClient;
    use webshelf::utils::load_config;
    
    // Load test configuration
    let config = load_config("config.toml", "development")
        .expect("Failed to load config");
    
    // Connect to test database
    let db = Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    
    // Run migrations
    webshelf::migrations::run_migrations(&db)
        .await
        .expect("Failed to run migrations");
    
    // Create Redis client (optional)
    let redis = RedisClient::open(config.redis_url.as_str())
        .ok();
    
    let state = AppState {
        db,
        redis,
        config: Arc::new(config),
    };
    
    use webshelf::middleware::auth::JwtSecret;
    use tower_http::cors::CorsLayer;
    use tower_http::trace::TraceLayer;
    use http::Method;
    
    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::PATCH, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any);
    
    // Build test router with same middleware stack as main app
    Router::new()
        .nest("/api", api_routes())
        .nest("/api/public/auth", auth_routes())
        .layer(axum::Extension(JwtSecret(state.config.jwt_secret.clone())))
        .layer(axum::middleware::from_fn(webshelf::middleware::panic::panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

// Helper to extract JSON body
async fn body_to_json(body: Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
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
        "password": "Password123",
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
        "password": "Password123",
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
    
    // Create user
    let payload = json!({
        "email": format!("createget_user_{}@example.com", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()),
        "password": "Password123",
        "name": "Create Get Test"
    });
    
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    let user_id = body["id"].as_str().unwrap();
    let expected_email = body["email"].as_str().unwrap(); // Get the actual email that was created
        
    // Get user
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/users/{}", user_id))
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
    let email = format!("conflict_test_{}@example.com", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let payload1 = json!({
        "email": &email,
        "password": "Password123",
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
