//! 密码重置模块集成测试
//!
//! - `forgot_password`：服务端对未知邮箱 / 已知邮箱均 200 兜底（anti-enumeration）；
//!   唯一真实错误码是 503（邮件服务未配置）。
//! - `reset_password`：服务端把 "token 不存在 / 已过期 / 错误 / 已被消费 /
//!   暴力尝试上限 / 弱密码" 全部统一 400 + 通用文案（anti-enumeration +
//!   凭证探测防护）。成功路径会签发新 JWT。

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

mod common;
use common::{create_test_client, fixtures};

/// 模拟合法的 6 位重置验证码。
const VALID_RESET_CODE: &str = "483921";

// ──────────────────────────────────────────────
//  Forgot password
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_forgot_password_success_known_email() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/forgot-password"))
        .and(body_json(serde_json::json!({
            "email": fixtures::TEST_EMAIL,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": "If that email is registered, a reset code has been sent",
        })))
        .mount(&mock_server)
        .await;

    let resp = client.forgot_password(fixtures::TEST_EMAIL).await.unwrap();
    assert_eq!(
        resp.message,
        "If that email is registered, a reset code has been sent"
    );
}

#[tokio::test]
async fn test_forgot_password_success_unknown_email_anti_enumeration() {
    // 服务端对未知邮箱也走 Argon2 dummy hash 恒定分支，**必须**返回 200。
    // 客户端无需也无法区分"邮箱是否存在"。
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/forgot-password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": "If that email is registered, a reset code has been sent",
        })))
        .mount(&mock_server)
        .await;

    let resp = client.forgot_password("nobody@example.com").await.unwrap();
    // 同样的 200 + 同样的通用文案 —— 防止 UI 层把不同文案拼起来当泄漏信号。
    assert_eq!(
        resp.message,
        "If that email is registered, a reset code has been sent"
    );
}

#[tokio::test]
async fn test_forgot_password_email_service_unavailable() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/forgot-password"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "error": "service_unavailable",
            "message": "Password reset is currently unavailable",
        })))
        .mount(&mock_server)
        .await;

    let result = client.forgot_password(fixtures::TEST_EMAIL).await;

    // 503 走 from_status 的 500..=599 分支，归为 ServerError 而非 Other。
    // 客户端 humanize_error 须兼容两种变体。
    match result.unwrap_err() {
        client_api::ClientError::ServerError(503, msg)
        | client_api::ClientError::Other(503, msg) => {
            assert!(msg.contains("unavailable") || msg.contains("service_unavailable"));
        }
        other => panic!(
            "Expected ServerError(503, ...) or Other(503, ...), got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_forgot_password_validation_error() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/forgot-password"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "validation_error",
            "message": "email: invalid format",
        })))
        .mount(&mock_server)
        .await;

    let result = client.forgot_password("not-an-email").await;
    assert!(matches!(
        result.unwrap_err(),
        client_api::ClientError::Other(400, _)
    ));
}

// ──────────────────────────────────────────────
//  Reset password
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_reset_password_success_issues_fresh_jwt() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/reset-password"))
        .and(body_json(serde_json::json!({
            "email": fixtures::TEST_EMAIL,
            "code": VALID_RESET_CODE,
            "new_password": "NewPass456!",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": "Password reset successfully",
            "token": fixtures::TEST_TOKEN,
            "token_type": "Bearer",
            "expires_in": 3600,
            "user_id": fixtures::TEST_USER_ID,
            "role": "user",
        })))
        .mount(&mock_server)
        .await;

    let resp = client
        .reset_password(fixtures::TEST_EMAIL, VALID_RESET_CODE, "NewPass456!")
        .await
        .unwrap();

    assert_eq!(resp.message, "Password reset successfully");
    assert_eq!(resp.token, fixtures::TEST_TOKEN);
    assert_eq!(resp.token_type, "Bearer");
    assert_eq!(resp.expires_in, 3600);
    assert_eq!(resp.user_id, fixtures::TEST_USER_ID);
    assert_eq!(resp.role, "user");
}

#[tokio::test]
async fn test_reset_password_invalid_code_generic_400() {
    // 服务端对"验证码不存在 / 错误 / 已过期 / 已被消费"统一返回 400。
    // 客户端必须只展示通用文案，不区分这些分支。
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/reset-password"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "bad_request",
            "message": "Invalid or expired reset code",
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .reset_password(fixtures::TEST_EMAIL, "000000", "NewPass456!")
        .await;

    match result.unwrap_err() {
        client_api::ClientError::Other(400, msg) => {
            assert!(msg.contains("Invalid") || msg.contains("expired"));
        }
        other => panic!("Expected Other(400, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_reset_password_weak_password() {
    // 弱密码应被服务端拒绝（与注册/改密的密码复杂度策略一致）。
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/reset-password"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "bad_request",
            "message": "Password too weak",
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .reset_password(fixtures::TEST_EMAIL, VALID_RESET_CODE, "weak")
        .await;

    match result.unwrap_err() {
        client_api::ClientError::Other(400, msg) => {
            assert!(msg.contains("weak") || msg.contains("validation_error"));
        }
        other => panic!("Expected Other(400, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_reset_password_brute_force_lockout() {
    // 暴力尝试上限达 5 次后，服务端把后续所有错误（含此分支）也归入统一 400。
    // 客户端不应区分"密码错误太多次"和"token 无效"——否则就是给攻击者反馈信号。
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/reset-password"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "bad_request",
            "message": "Invalid or expired reset code",
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .reset_password(fixtures::TEST_EMAIL, "111111", "NewPass456!")
        .await;

    // 与"invalid code"分支的 status 码完全一致 —— 反 enumeration 一致性检查。
    assert!(matches!(
        result.unwrap_err(),
        client_api::ClientError::Other(400, _)
    ));
}
