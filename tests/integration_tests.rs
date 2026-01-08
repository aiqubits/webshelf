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
    
    // Create Redis client
    let redis = RedisClient::open(config.redis_url.as_str())
        .expect("Failed to create Redis client");
    
    let state = AppState {
        db,
        redis,
        config: Arc::new(config),
    };
    
    // Build test router
    Router::new()
        .nest("/api", api_routes())
        .nest("/api/public/auth", auth_routes())
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
        "email": "test@example.com",
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
        "email": "createget@example.com",
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
    assert_eq!(body["email"], "createget@example.com");
}
