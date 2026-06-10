//! 管理员用户管理模块集成测试
//!
//! 测试 CRUD 操作：list / create / get / update / delete

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

mod common;
use common::{create_test_client, fixtures};

const ID1: &str = "11111111-1111-4111-8111-111111111111";
const ID2: &str = "22222222-2222-4222-8222-222222222222";
const ID3: &str = "33333333-3333-4333-8333-333333333333";
const BASE_TS: &str = "2024-01-15T08:00:00Z";
const UPDATED_TS: &str = "2024-06-09T12:00:00Z";

fn setup_admin_client(client: &client_api::Client) {
    client.set_token(fixtures::TEST_TOKEN);
}

// ──────────────────────────────────────────────
//  List users
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_list_users_empty() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    Mock::given(method("GET"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [],
            "total": 0,
            "page": 1,
            "per_page": 10,
            "total_pages": 0,
        })))
        .mount(&mock_server)
        .await;

    let resp = client.list_users(1, 10).await.unwrap();
    assert!(resp.items.is_empty());
    assert_eq!(resp.total, 0);
    assert_eq!(resp.total_pages, 0);
}

#[tokio::test]
async fn test_list_users_with_data() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    Mock::given(method("GET"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [
                fixtures::user_json(ID1, "user1@example.com", "Alice", "user", BASE_TS, BASE_TS),
                fixtures::user_json(ID2, "user2@example.com", "Bob", "admin", BASE_TS, BASE_TS),
            ],
            "total": 2,
            "page": 1,
            "per_page": 10,
            "total_pages": 1,
        })))
        .mount(&mock_server)
        .await;

    let resp = client.list_users(1, 10).await.unwrap();
    assert_eq!(resp.items.len(), 2);
    assert_eq!(resp.total, 2);
    assert_eq!(resp.items[0].name, "Alice");
    assert_eq!(resp.items[1].role, "admin");
}

#[tokio::test]
async fn test_list_users_pagination() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    Mock::given(method("GET"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [fixtures::user_json(ID3, "u3@example.com", "Page2 User", "user", BASE_TS, BASE_TS)],
            "total": 11,
            "page": 2,
            "per_page": 10,
            "total_pages": 2,
        })))
        .mount(&mock_server)
        .await;

    let resp = client.list_users(2, 10).await.unwrap();
    assert_eq!(resp.page, 2);
    assert_eq!(resp.total_pages, 2);
    assert_eq!(resp.items.len(), 1);
}

// ──────────────────────────────────────────────
//  Create user
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_create_user_success() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    Mock::given(method("POST"))
        .and(path("/api/users"))
        .and(body_json(serde_json::json!({
            "email": "new@example.com",
            "password": "SecurePass123!",
            "name": "New User",
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixtures::user_json(
                fixtures::TEST_USER_ID,
                "new@example.com",
                "New User",
                "user",
                BASE_TS,
                BASE_TS,
            )),
        )
        .mount(&mock_server)
        .await;

    let user = client
        .create_user("new@example.com", "SecurePass123!", "New User")
        .await
        .unwrap();

    assert_eq!(user.email, "new@example.com");
    assert_eq!(user.name, "New User");
    assert_eq!(user.role, "user");
}

#[tokio::test]
async fn test_create_user_duplicate_email() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    Mock::given(method("POST"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": "conflict",
            "message": "Email already registered",
        })))
        .mount(&mock_server)
        .await;

    let result = client
        .create_user(fixtures::TEST_EMAIL, "SecurePass123!", "Dup")
        .await;

    assert!(result.is_err());
}

// ──────────────────────────────────────────────
//  Get user
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_get_user_success() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    let id = uuid::Uuid::parse_str(fixtures::TEST_USER_ID).unwrap();

    Mock::given(method("GET"))
        .and(path(format!("/api/users/{}", id)))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixtures::user_json(
                fixtures::TEST_USER_ID,
                fixtures::TEST_EMAIL,
                fixtures::TEST_NAME,
                "user",
                BASE_TS,
                BASE_TS,
            )),
        )
        .mount(&mock_server)
        .await;

    let user = client.get_user(id).await.unwrap();
    assert_eq!(user.id.to_string(), fixtures::TEST_USER_ID);
    assert_eq!(user.email, fixtures::TEST_EMAIL);
}

#[tokio::test]
async fn test_get_user_not_found() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    let id = uuid::Uuid::parse_str(fixtures::TEST_USER_ID).unwrap();

    Mock::given(method("GET"))
        .and(path(format!("/api/users/{}", id)))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "not_found",
            "message": "User not found",
        })))
        .mount(&mock_server)
        .await;

    let result = client.get_user(id).await;
    assert!(result.is_err());
}

// ──────────────────────────────────────────────
//  Update user
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_update_user_success() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    let id = uuid::Uuid::parse_str(fixtures::TEST_USER_ID).unwrap();

    Mock::given(method("PUT"))
        .and(path(format!("/api/users/{}", id)))
        .and(body_json(serde_json::json!({
            "email": "updated@example.com",
            "name": "Updated Name",
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixtures::user_json(
                fixtures::TEST_USER_ID,
                "updated@example.com",
                "Updated Name",
                "user",
                BASE_TS,
                UPDATED_TS,
            )),
        )
        .mount(&mock_server)
        .await;

    let user = client
        .update_user(
            id,
            Some("updated@example.com".into()),
            Some("Updated Name".into()),
            None,
        )
        .await
        .unwrap();

    assert_eq!(user.email, "updated@example.com");
    assert_eq!(user.name, "Updated Name");
}

#[tokio::test]
async fn test_update_user_role_only() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    let id = uuid::Uuid::parse_str(fixtures::TEST_USER_ID).unwrap();

    Mock::given(method("PUT"))
        .and(path(format!("/api/users/{}", id)))
        .and(body_json(serde_json::json!({
            "role": "admin",
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixtures::user_json(
                fixtures::TEST_USER_ID,
                fixtures::TEST_EMAIL,
                fixtures::TEST_NAME,
                "admin",
                BASE_TS,
                UPDATED_TS,
            )),
        )
        .mount(&mock_server)
        .await;

    let user = client
        .update_user(id, None, None, Some("admin".into()))
        .await
        .unwrap();

    assert_eq!(user.role, "admin");
}

#[tokio::test]
async fn test_update_user_not_found() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    let id = uuid::Uuid::parse_str(fixtures::TEST_USER_ID).unwrap();

    Mock::given(method("PUT"))
        .and(path(format!("/api/users/{}", id)))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "not_found",
            "message": "User not found",
        })))
        .mount(&mock_server)
        .await;

    let result = client.update_user(id, None, Some("New".into()), None).await;
    assert!(result.is_err());
}

// ──────────────────────────────────────────────
//  Delete user
// ──────────────────────────────────────────────

#[tokio::test]
async fn test_delete_user_success() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    let id = uuid::Uuid::parse_str(fixtures::TEST_USER_ID).unwrap();

    Mock::given(method("DELETE"))
        .and(path(format!("/api/users/{}", id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": "User deleted successfully",
        })))
        .mount(&mock_server)
        .await;

    let resp = client.delete_user(id).await.unwrap();
    assert_eq!(resp.message, "User deleted successfully");
}

#[tokio::test]
async fn test_delete_user_not_found() {
    let (client, mock_server) = create_test_client().await;
    setup_admin_client(&client);

    let id = uuid::Uuid::parse_str(fixtures::TEST_USER_ID).unwrap();

    Mock::given(method("DELETE"))
        .and(path(format!("/api/users/{}", id)))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "not_found",
            "message": "User not found",
        })))
        .mount(&mock_server)
        .await;

    let result = client.delete_user(id).await;
    assert!(result.is_err());
}
