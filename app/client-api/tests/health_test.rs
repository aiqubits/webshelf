//! 健康检查模块集成测试

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

mod common;
use common::create_test_client;

#[tokio::test]
async fn test_health_check_ok() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "version": "0.1.0",
        })))
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;

    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.status, "ok");
    assert_eq!(resp.version, "0.1.0");
}

#[tokio::test]
async fn test_health_check_returns_version() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "version": "1.5.2",
        })))
        .mount(&mock_server)
        .await;

    let resp = client.health_check().await.unwrap();
    assert_eq!(resp.version, "1.5.2");
}

#[tokio::test]
async fn test_health_check_down() {
    let (client, mock_server) = create_test_client().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "error": "service_unavailable",
            "message": "Database connection failed",
        })))
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_health_check_no_auth_required() {
    // 健康检查无需携带认证 token
    let (client, mock_server) = create_test_client().await;

    // 设置一个 token 来验证健康检查不依赖它
    client.set_token("some-token");

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "version": "0.1.0",
        })))
        .mount(&mock_server)
        .await;

    let result = client.health_check().await;
    assert!(result.is_ok());
}
