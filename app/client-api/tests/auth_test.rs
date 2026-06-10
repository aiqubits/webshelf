//! 认证模块集成测试
//!
//! 使用 Wiremock 模拟后端服务，无需启动真实服务器。

use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use client_api::ClientError;

mod common;
use common::{create_test_client, fixtures};

// ──────────────────────────────────────────────
//  Login tests
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_login_success() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": fixtures::TEST_TOKEN,
            "token_type": "Bearer",
            "expires_in": 3600,
            "user_id": fixtures::TEST_USER_ID,
            "role": "user",
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .login(fixtures::TEST_EMAIL, fixtures::TEST_PASSWORD)
        .await;

    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.token, fixtures::TEST_TOKEN);
    assert_eq!(resp.token_type, "Bearer");
    assert_eq!(resp.expires_in, 3600);
    assert_eq!(resp.user_id, fixtures::TEST_USER_ID);
    assert_eq!(resp.role, "user");
}

#[tokio::test]
async fn test_login_admin_role() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .and(body_json(serde_json::json!({
            "email": "admin@example.com",
            "password": "admin123!@#",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": fixtures::TEST_TOKEN,
            "token_type": "Bearer",
            "expires_in": 7200,
            "user_id": fixtures::TEST_USER_ID,
            "role": "admin",
        })))
        .mount(&mock_server)
        .await;

    let result = client.login("admin@example.com", "admin123!@#").await;

    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.role, "admin");
    assert_eq!(resp.expires_in, 7200);
}

#[tokio::test]
async fn test_login_invalid_credentials() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "unauthorized",
            "message": "Invalid email or password",
        })))
        .mount(&mock_server)
        .await;

    let result = client.login("wrong@example.com", "wrongpassword").await;

    match result.unwrap_err() {
        ClientError::Other(401, msg) => {
            assert!(msg.contains("unauthorized") || msg.contains("Invalid"));
        }
        other => panic!("Expected Other(401, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_login_body_matches_request() {
    let (client, mock_server) = create_test_client().await;

    // 验证客户端发送了正确的 JSON 请求体
    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .and(body_json(serde_json::json!({
            "email": fixtures::TEST_EMAIL,
            "password": "my-password",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": fixtures::TEST_TOKEN,
            "token_type": "Bearer",
            "expires_in": 3600,
            "user_id": fixtures::TEST_USER_ID,
            "role": "user",
        })))
        .mount(&mock_server)
        .await;

    let result = client.login(fixtures::TEST_EMAIL, "my-password").await;
    assert!(result.is_ok());
}

// ──────────────────────────────────────────────
//  Register tests
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_register_success() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/register"))
        .and(body_json(serde_json::json!({
            "email": "newuser@example.com",
            "password": "SecurePass123!",
            "name": "New User",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": "User registered successfully",
            "user_id": fixtures::TEST_USER_ID,
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .register("newuser@example.com", "SecurePass123!", "New User")
        .await;

    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.message, "User registered successfully");
    assert_eq!(resp.user_id, fixtures::TEST_USER_ID);
}

#[tokio::test]
async fn test_register_email_conflict() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/register"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": "conflict",
            "message": "Email already registered",
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .register(fixtures::TEST_EMAIL, "SecurePass123!", "Existing User")
        .await;

    match result.unwrap_err() {
        ClientError::Other(409, msg) => {
            assert!(msg.contains("conflict") || msg.contains("already registered"));
        }
        other => panic!("Expected Other(409, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_register_validation_error() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/register"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "validation_error",
            "message": "password must be at least 8 characters",
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .register("test@example.com", "short", "Test User")
        .await;

    match result.unwrap_err() {
        ClientError::Other(400, msg) => {
            assert!(msg.contains("validation_error") || msg.contains("password"));
        }
        other => panic!("Expected Other(400, ...), got {:?}", other),
    }
}

// ──────────────────────────────────────────────
//  Token flow: login → set_token → authenticated request
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_login_and_use_token_for_authenticated_request() {
    let (client, mock_server) = create_test_client().await;

    // 1. Mock login
    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": fixtures::TEST_TOKEN,
            "token_type": "Bearer",
            "expires_in": 3600,
            "user_id": fixtures::TEST_USER_ID,
            "role": "admin",
        })))
        .mount(&mock_server)
        .await;

    let login = client
        .login(fixtures::TEST_EMAIL, fixtures::TEST_PASSWORD)
        .await
        .unwrap();
    client.set_token(&login.token);
    assert!(client.is_authenticated());

    // 2. Mock authenticated request (list users) — 验证 Authorization 头
    Mock::given(method("GET"))
        .and(path("/api/users"))
        .and(header(
            "Authorization",
            format!("Bearer {}", fixtures::TEST_TOKEN),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [fixtures::user_json(
                fixtures::TEST_USER_ID,
                fixtures::TEST_EMAIL,
                "Admin User",
                "admin",
                "2024-01-01T00:00:00Z",
                "2024-06-01T00:00:00Z",
            )],
            "total": 1,
            "page": 1,
            "per_page": 10,
            "total_pages": 1,
        })))
        .mount(&mock_server)
        .await;

    let users = client.list_users(1, 10).await.unwrap();
    assert_eq!(users.items.len(), 1);
    assert_eq!(users.items[0].email, fixtures::TEST_EMAIL);

    // 3. Logout
    client.clear_token();
    assert!(!client.is_authenticated());
}
