#![cfg(feature = "webshelf-salvo")]

//! Salvo 模式登出所有设备集成测试。
//!
//! 验证 token_version 机制在 salvo 运行时下的正确性：
//! 1. 旧 JWT 在 logout-all 后被拒绝
//! 2. 未认证请求被拦截
//! 3. 当前会话的请求本身成功

mod common;
use common::salvo::{self, TestServer};

async fn create_server() -> TestServer {
    salvo::create_test_server().await
}

/// Register and login, return JWT token.
async fn register_and_login(server: &TestServer, email: &str) -> String {
    salvo::register_and_login(server, email).await
}

/// Call POST /api/users/me/logout-all and return status.
async fn call_logout_all(server: &TestServer, token: &str) -> reqwest::StatusCode {
    let payload = serde_json::json!({});
    let (status, _) = salvo::post(
        server,
        "/api/users/me/logout-all",
        Some(token),
        Some(&payload),
    )
    .await;
    status
}

/// Check if a token is valid by calling GET /api/users/me.
async fn check_token_valid(server: &TestServer, token: &str) -> reqwest::StatusCode {
    let (status, _) = salvo::get(server, "/api/users/me", Some(token)).await;
    status
}

/// (1) 正常调用后旧 JWT 被拒绝返回 401。
#[tokio::test]
async fn test_old_jwt_rejected_after_logout_all() {
    let server = create_server().await;
    let email = common::unique_email("salvo_lo_old_jwt");
    let old_token = register_and_login(&server, &email).await;

    // Verify old JWT is valid before logout-all
    assert_eq!(
        check_token_valid(&server, &old_token).await,
        reqwest::StatusCode::OK,
        "Old JWT should be valid before logout-all"
    );

    // Call logout-all
    let status = call_logout_all(&server, &old_token).await;
    assert_eq!(status, reqwest::StatusCode::OK, "logout-all should succeed");

    // Old JWT should be rejected
    assert_eq!(
        check_token_valid(&server, &old_token).await,
        reqwest::StatusCode::UNAUTHORIZED,
        "Old JWT should be rejected after logout-all"
    );

    // Re-login should get a new valid JWT
    let login_payload = serde_json::json!({
        "email": email,
        "password": "Password123!"
    });
    let (status, body) = salvo::post_json(&server, "/api/public/auth/login", &login_payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let new_token = body["token"].as_str().unwrap();

    assert_eq!(
        check_token_valid(&server, new_token).await,
        reqwest::StatusCode::OK,
        "New JWT obtained after logout-all should be valid"
    );
}

/// (2) 未认证用户返回 401。
#[tokio::test]
async fn test_logout_all_unauthenticated() {
    let server = create_server().await;

    // Without auth header
    let payload = serde_json::json!({});
    let (status, _) = salvo::post(&server, "/api/users/me/logout-all", None, Some(&payload)).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

/// (3) 当前会话的请求本身成功返回 200。
#[tokio::test]
async fn test_logout_all_current_session_succeeds() {
    let server = create_server().await;
    let email = common::unique_email("salvo_lo_self");
    let token = register_and_login(&server, &email).await;

    let payload = serde_json::json!({});
    let (status, body) = salvo::post(
        &server,
        "/api/users/me/logout-all",
        Some(&token),
        Some(&payload),
    )
    .await;

    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "The current session's logout-all request itself should return 200"
    );
    assert_eq!(body["message"], "Logged out from all devices");
}
