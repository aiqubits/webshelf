//! 错误处理集成测试
//!
//! 测试各种 HTTP 错误状态码的处理和重试行为。

use std::time::Duration;

use client_api::{ClientConfig, ClientError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

mod common;
use common::{create_test_client, fixtures};

// ──────────────────────────────────────────────
//  HTTP error status codes
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_401_unauthorized() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "unauthorized",
            "message": "Invalid email or password",
        })))
        .mount(&mock_server)
        .await;

    let err = client
        .login("test@example.com", "wrong", false)
        .await
        .unwrap_err();

    match err {
        ClientError::Other(status, msg) => {
            assert_eq!(status, 401);
            assert!(msg.contains("unauthorized") || msg.contains("Invalid"));
        }
        other => panic!("Expected Other(401, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_404_not_found() {
    let (client, mock_server) = create_test_client().await;
    client.set_token(fixtures::TEST_TOKEN);

    let id = fixtures::TEST_USER_ID.to_string();

    Mock::given(method("GET"))
        .and(path(format!("/api/users/{}", id)))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "not_found",
            "message": "User not found",
        })))
        .mount(&mock_server)
        .await;

    let err = client.get_user(id).await.unwrap_err();

    match err {
        ClientError::Other(status, msg) => {
            assert_eq!(status, 404);
            assert!(msg.contains("not_found") || msg.contains("User not found"));
        }
        other => panic!("Expected Other(404, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_409_conflict() {
    let (client, mock_server) = create_test_client().await;
    client.set_token(fixtures::TEST_TOKEN);

    Mock::given(method("POST"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": "conflict",
            "message": "Email already registered",
        })))
        .mount(&mock_server)
        .await;

    let err = client
        .create_user("dup@example.com", "SecurePass123!", "Dup", None)
        .await
        .unwrap_err();

    match err {
        ClientError::Other(status, msg) => {
            assert_eq!(status, 409);
            assert!(msg.contains("conflict"));
        }
        other => panic!("Expected Other(409, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_500_server_error() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": "internal_error",
            "message": "Internal server error",
        })))
        .mount(&mock_server)
        .await;

    let err = client.health_check().await.unwrap_err();

    match err {
        ClientError::ServerError(status, msg) => {
            assert_eq!(status, 500);
            assert!(msg.contains("Internal") || msg.contains("internal"));
        }
        other => panic!("Expected ServerError(500, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_503_service_unavailable() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("Retry-After", "120")
                .set_body_json(serde_json::json!({
                    "error": "service_unavailable",
                    "message": "Database connection failed",
                })),
        )
        .mount(&mock_server)
        .await;

    let err = client.health_check().await.unwrap_err();

    match err {
        ClientError::ServerError(status, _) => {
            assert_eq!(status, 503);
        }
        other => panic!("Expected ServerError(503, ...), got {:?}", other),
    }
}

#[tokio::test]
async fn test_429_rate_limited() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "60")
                .set_body_json(serde_json::json!({
                    "error": "rate_limited",
                    "message": "Too many requests",
                })),
        )
        .mount(&mock_server)
        .await;

    let err = client
        .login("test@example.com", "password", false)
        .await
        .unwrap_err();

    match err {
        ClientError::RateLimited(msg) => {
            assert!(msg.contains("Too many requests"));
        }
        other => panic!("Expected RateLimited, got {:?}", other),
    }
}

// ──────────────────────────────────────────────
//  Network / timeout errors (via wiremock delay)
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_timeout() {
    let (client, mock_server) = create_test_client().await;

    // Mock 响应延迟远超客户端超时时间（10s），触发超时
    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(60))
                .set_body_json(serde_json::json!({"status": "ok", "version": "1.0"})),
        )
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ClientError::Network(_) => {} // 超时归类为网络错误
        other => panic!("Expected Network error, got {:?}", other),
    }
}

// ──────────────────────────────────────────────
//  Invalid JSON response
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_invalid_json_response() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json {{{{}}"))
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(result.is_err());
}

// ──────────────────────────────────────────────
//  Structured error body parsing
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_structured_error_body_raw_json() {
    let (client, mock_server) = create_test_client().await;

    // 后端返回结构化错误 {"error": "...", "message": "..."}
    // Client::handle_response 不再预先格式化为 "[code] message"，
    // 而是将原始 JSON body 原样传递给调用方，由 humanize_error 自行解析。
    Mock::given(method("POST"))
        .and(path("/api/public/auth/login"))
        .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
            "error": "validation_error",
            "message": "email must be a valid email address",
        })))
        .mount(&mock_server)
        .await;

    let err = client
        .login("not-an-email", "password123", false)
        .await
        .unwrap_err();

    match err {
        ClientError::Other(status, msg) => {
            assert_eq!(status, 422);
            // 验证传递的是原始 JSON body，而非预格式化的 "[code] message"。
            // 调用方 (如 humanize_error) 可自行反序列化 ErrorBody。
            assert!(msg.starts_with('{'), "Expected raw JSON body, got: {msg:?}");
            assert!(
                msg.contains(r#""validation_error""#),
                "Raw JSON should contain error code, got: {msg:?}"
            );
            assert!(
                msg.contains(r#""email must be a valid email address""#),
                "Raw JSON should contain error message, got: {msg:?}"
            );
            // 额外验证：不应包含旧格式的方括号前缀
            assert!(
                !msg.starts_with('['),
                "Should NOT contain pre-formatted '[code]' pattern, got: {msg:?}"
            );
        }
        other => panic!(
            "Expected Other(422, ...) with raw JSON body, got {:?}",
            other
        ),
    }
}

// ──────────────────────────────────────────────
//  Retry flow — 端到端重试行为测试
// ──────────────────────────────────────────────

use std::sync::Mutex;

/// 有状态响应器：第 N 次调用返回不同的响应。
struct SequentialResponder {
    responses: Vec<ResponseTemplate>,
    call_count: Mutex<usize>,
}

impl Respond for SequentialResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let mut count = self.call_count.lock().unwrap();
        let idx = *count;
        *count += 1;
        self.responses
            .get(idx)
            .cloned()
            .unwrap_or_else(|| self.responses.last().cloned().unwrap())
    }
}

/// 第一次请求返回 503 → 重试 → 第二次返回 200 成功
#[tokio::test]
async fn test_retry_503_then_200_succeeds() {
    let mock_server = wiremock::MockServer::start().await;
    let config = ClientConfig::new(mock_server.uri())
        .with_max_retries(2)
        .with_timeout(10);
    let client = client_api::Client::new(config).unwrap();

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(SequentialResponder {
            responses: vec![
                ResponseTemplate::new(503), // 第 1 次：触发重试
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "status": "ok",
                    "version": "1.0",
                })), // 第 2 次：成功
            ],
            call_count: Mutex::new(0),
        })
        .expect(2)
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(
        result.is_ok(),
        "Retry should succeed on second attempt, got: {:?}",
        result.err()
    );
    let resp = result.unwrap();
    assert_eq!(resp.status, "ok");
}

/// 所有重试均返回 503 → 重试耗尽 → 返回错误
#[tokio::test]
async fn test_retry_503_exhausted() {
    let mock_server = wiremock::MockServer::start().await;
    let config = ClientConfig::new(mock_server.uri())
        .with_max_retries(2)
        .with_timeout(10);
    let client = client_api::Client::new(config).unwrap();

    // 全部 3 次请求（1 初始 + 2 重试）均返回 503
    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(result.is_err(), "Should fail after exhausting all retries");

    match result.unwrap_err() {
        ClientError::ServerError(status, _) => {
            assert_eq!(status, 503);
        }
        other => panic!("Expected ServerError(503), got {:?}", other),
    }
}

/// 第一次 503 → 第二次 429 → 第三次 200（不同可重试错误混合）
#[tokio::test]
async fn test_retry_mixed_errors_then_success() {
    let mock_server = wiremock::MockServer::start().await;
    let config = ClientConfig::new(mock_server.uri())
        .with_max_retries(3)
        .with_timeout(10);
    let client = client_api::Client::new(config).unwrap();

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(SequentialResponder {
            responses: vec![
                ResponseTemplate::new(503), // 可重试
                ResponseTemplate::new(429), // 可重试
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "status": "ok",
                    "version": "1.0",
                })), // 第三次成功
            ],
            call_count: Mutex::new(0),
        })
        .expect(3)
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(
        result.is_ok(),
        "Retry should handle mixed errors, got: {:?}",
        result.err()
    );
}

/// 4xx 错误不重试，直接返回
#[tokio::test]
async fn test_no_retry_on_4xx() {
    let mock_server = wiremock::MockServer::start().await;
    let config = ClientConfig::new(mock_server.uri())
        .with_max_retries(3)
        .with_timeout(10);
    let client = client_api::Client::new(config).unwrap();

    // 返回 404 — 不应重试，mock 期望仅命中 1 次
    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "not_found",
            "message": "Resource not found",
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        ClientError::Other(status, _) => {
            assert_eq!(status, 404);
        }
        other => panic!("Expected Other(404), got {:?}", other),
    }
}
