//! 自助服务模块集成测试
//!
//! - `change_password`：自改密码后 `new_token` 必须被消费（服务端原子地
//!   `token_version += 1`，旧 JWT 永久失效 —— 客户端必须用新 token 替换）
//! - `get_me`：当前登录用户的资料读取（用于会话恢复后填充 name/email）

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

mod common;
use common::{create_test_client, fixtures};

const ID: &str = "550e8400-e29b-41d4-a716-446655440000";
const TS: &str = "2024-06-15T10:00:00Z";

// ──────────────────────────────────────────────
//  Change password
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_change_password_success_returns_new_token() {
    let (client, mock_server) = create_test_client().await;
    client.set_token(fixtures::TEST_TOKEN);

    Mock::given(method("POST"))
        .and(path("/api/users/me/password"))
        .and(body_json(serde_json::json!({
            "current_password": "OldPass123!",
            "new_password": "NewPass456!",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": "Password changed successfully",
            "new_token": fixtures::TEST_TOKEN,
        })))
        .mount(&mock_server)
        .await;

    let resp = client
        .change_password("OldPass123!", "NewPass456!")
        .await
        .unwrap();

    assert_eq!(resp.message, "Password changed successfully");
    // B1 回归保护：客户端必须能拿到 new_token 以便轮转本地缓存。
    // 如果未来有人不小心把 new_token 字段从 ChangePasswordResponse 中移除，
    // 这个断言会编译失败/反序列化失败，第一时间暴露问题。
    assert_eq!(resp.new_token, fixtures::TEST_TOKEN);
    assert!(
        !resp.new_token.is_empty(),
        "new_token must be non-empty for JWT rotation"
    );
}

#[tokio::test]
async fn test_change_password_wrong_current_password() {
    let (client, mock_server) = create_test_client().await;
    client.set_token(fixtures::TEST_TOKEN);

    Mock::given(method("POST"))
        .and(path("/api/users/me/password"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "unauthorized",
            "message": "Current password is incorrect",
        })))
        .mount(&mock_server)
        .await;

    let result = client.change_password("WrongOldPass!", "NewPass456!").await;

    match result.unwrap_err() {
        client_api::ClientError::Other(401, msg) => {
            assert!(msg.contains("Current password") || msg.contains("unauthorized"));
        }
        other => panic!("Expected Other(401, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_change_password_weak_new_password() {
    let (client, mock_server) = create_test_client().await;
    client.set_token(fixtures::TEST_TOKEN);

    Mock::given(method("POST"))
        .and(path("/api/users/me/password"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "bad_request",
            "message": "Password too weak",
        })))
        .mount(&mock_server)
        .await;

    let result = client.change_password("OldPass123!", "weak").await;

    match result.unwrap_err() {
        client_api::ClientError::Other(400, msg) => {
            assert!(msg.contains("weak") || msg.contains("validation_error"));
        }
        other => panic!("Expected Other(400, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_change_password_validation_error() {
    let (client, mock_server) = create_test_client().await;
    client.set_token(fixtures::TEST_TOKEN);

    Mock::given(method("POST"))
        .and(path("/api/users/me/password"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "validation_error",
            "message": "current_password must be at least 1 characters",
        })))
        .mount(&mock_server)
        .await;

    let result = client.change_password("", "NewPass456!").await;
    assert!(matches!(
        result.unwrap_err(),
        client_api::ClientError::Other(400, _)
    ));
}

// ──────────────────────────────────────────────
//  Get me
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_get_me_success() {
    let (client, mock_server) = create_test_client().await;
    client.set_token(fixtures::TEST_TOKEN);

    Mock::given(method("GET"))
        .and(path("/api/users/me"))
        .and(wiremock::matchers::header(
            "Authorization",
            format!("Bearer {}", fixtures::TEST_TOKEN),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixtures::user_json(
                ID,
                fixtures::TEST_EMAIL,
                fixtures::TEST_NAME,
                "user",
                TS,
                TS,
            )),
        )
        .mount(&mock_server)
        .await;

    let me = client.get_me().await.unwrap();

    assert_eq!(me.id.to_string(), ID);
    assert_eq!(me.email, fixtures::TEST_EMAIL);
    assert_eq!(me.name, fixtures::TEST_NAME);
    assert_eq!(me.role, "user");
}

#[tokio::test]
async fn test_get_me_unauthorized() {
    let (client, mock_server) = create_test_client().await;
    // 即便 set 了 token，服务端仍可能因 token_version 升级而 401。
    client.set_token("expired-or-revoked-token");

    Mock::given(method("GET"))
        .and(path("/api/users/me"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "unauthorized",
            "message": "Invalid or expired token",
        })))
        .mount(&mock_server)
        .await;

    let result = client.get_me().await;
    assert!(matches!(
        result.unwrap_err(),
        client_api::ClientError::Other(401, _)
    ));
}
