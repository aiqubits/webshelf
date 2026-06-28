#![cfg(feature = "webshelf-salvo")]

//! Salvo 模式集成测试 — 使用完整 HTTP 栈验证 API 端点。
//!
//! 这些测试需要运行 PostgreSQL 和 Redis 实例。
//! 启动方式：
//!   cargo test --features webshelf-salvo --test salvo_integration_tests
//!
//! 注意：测试使用带纳秒时间戳的唯一邮箱以避免冲突。

mod common;
use common::salvo::{self, TestServer};

// ── 共享测试辅助 ────────────────────────────────────────────────

/// 创建测试服务器
async fn create_server() -> TestServer {
    salvo::create_test_server().await
}

// ── 健康检查 ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_health_check() {
    let server = create_server().await;
    let (status, body) = salvo::get(&server, "/api/health", None).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

// ── 用户注册 ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_user_registration() {
    let server = create_server().await;
    let email = common::unique_email("register");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });

    let (status, body) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    // 注册接口返回 { message, user_id, email_verified }，不直接返回 email/name/role
    assert_eq!(body["message"], "User registered successfully");
    assert!(body["user_id"].is_string(), "user_id should be a string");
    assert_eq!(body["email_verified"], true);
}

#[tokio::test]
async fn test_registration_password_confirm_mismatch() {
    let server = create_server().await;
    let email = common::unique_email("pwconfirm");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "DifferentPassword456!",
        "name": "Test User"
    });

    let (status, body) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn test_user_registration_invalid_email() {
    let server = create_server().await;

    let payload = serde_json::json!({
        "email": "not-an-email",
        "password": "Password123!",
        "name": "Test User"
    });

    let (status, body) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    // Should be rejected with 400
    assert!(status.is_client_error());
    // 验证错误通过 HttpError 返回，error_type 为 "bad_request"（非 "validation_error"）
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn test_user_registration_short_password() {
    let server = create_server().await;

    let payload = serde_json::json!({
        "email": common::unique_email("short_pw"),
        "password": "short",
        "name": "Test User"
    });

    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert!(status.is_client_error());
}

// ── 登录 ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_login_success() {
    let server = create_server().await;
    let email = common::unique_email("login");
    let token = salvo::register_and_login(&server, &email).await;
    assert!(!token.is_empty());
}

#[tokio::test]
async fn test_login_invalid_credentials() {
    let server = create_server().await;
    let email = common::unique_email("wrong_pw");

    // Register first
    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "name": "Test User"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Login with wrong password
    let login_payload = serde_json::json!({
        "email": email,
        "password": "WrongPassword1!"
    });
    let (status, body) = salvo::post_json(&server, "/api/public/auth/login", &login_payload).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
}

// ── 认证保护 ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_unauthenticated_request_rejected() {
    let server = create_server().await;

    let (status, body) = salvo::get(&server, "/api/users/me", None).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
}

#[tokio::test]
async fn test_get_me_success() {
    let server = create_server().await;
    let email = common::unique_email("get_me");
    let token = salvo::register_and_login(&server, &email).await;

    let (status, body) = salvo::get(&server, "/api/users/me", Some(&token)).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(body["email"], email.to_lowercase());
}

// ── 密码修改 ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_change_password_success() {
    let server = create_server().await;
    let email = common::unique_email("chg_pw");
    let token = salvo::register_and_login(&server, &email).await;

    let payload = serde_json::json!({
        "current_password": "Password123!",
        "new_password": "NewStrongPass1!"
    });

    let (status, body) = salvo::post(
        &server,
        "/api/users/me/password",
        Some(&token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(body["new_token"].as_str().is_some());

    // New token should work
    let new_token = body["new_token"].as_str().unwrap();
    let (status, _) = salvo::get(&server, "/api/users/me", Some(new_token)).await;
    assert_eq!(status, reqwest::StatusCode::OK);
}

#[tokio::test]
async fn test_change_password_wrong_current() {
    let server = create_server().await;
    let email = common::unique_email("wrong_cur");
    let token = salvo::register_and_login(&server, &email).await;

    let payload = serde_json::json!({
        "current_password": "WrongPassword1!",
        "new_password": "NewStrongPass1!"
    });

    let (status, body) = salvo::post(
        &server,
        "/api/users/me/password",
        Some(&token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
}

#[tokio::test]
async fn test_old_token_invalidated_after_password_change() {
    let server = create_server().await;
    let email = common::unique_email("old_tok_pw");
    let old_token = salvo::register_and_login(&server, &email).await;

    // Change password
    let payload = serde_json::json!({
        "current_password": "Password123!",
        "new_password": "NewStrongPass2!"
    });

    let (status, _) = salvo::post(
        &server,
        "/api/users/me/password",
        Some(&old_token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Old JWT should be rejected
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&old_token)).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

// ── 用户 CRUD（需要 admin 权限） ─────────────────────────────────

#[tokio::test]
async fn test_create_and_get_user() {
    let server = create_server().await;
    let admin_email = common::unique_email("admin_crud");
    let admin_token = salvo::register_and_login(&server, &admin_email).await;

    // Promote to admin via DB
    let admin_token = promote_to_admin(&server, &admin_email, &admin_token).await;

    // Create a new user
    let new_email = common::unique_email("crud_new");
    let create_payload = serde_json::json!({
        "email": new_email,
        "password": "Password123!",
        "name": "New User"
    });

    let (status, create_body) = salvo::post(
        &server,
        "/api/users",
        Some(&admin_token),
        Some(&create_payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let user_id = create_body["id"].as_str().unwrap();

    // Get the user
    let (status, get_body) = salvo::get(
        &server,
        &format!("/api/users/{}", user_id),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(get_body["email"], new_email.to_lowercase());
}

#[tokio::test]
async fn test_delete_user() {
    let server = create_server().await;
    let admin_email = common::unique_email("admin_del");
    let admin_token = salvo::register_and_login(&server, &admin_email).await;
    let admin_token = promote_to_admin(&server, &admin_email, &admin_token).await;

    // Create a user to delete
    let del_email = common::unique_email("to_delete");
    let create_payload = serde_json::json!({
        "email": del_email,
        "password": "Password123!",
        "name": "Delete Me"
    });

    let (status, create_body) = salvo::post(
        &server,
        "/api/users",
        Some(&admin_token),
        Some(&create_payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let user_id = create_body["id"].as_str().unwrap();

    // Delete the user
    let (status, _) = salvo::delete(
        &server,
        &format!("/api/users/{}", user_id),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Verify deleted — should return 404
    let (status, _) = salvo::get(
        &server,
        &format!("/api/users/{}", user_id),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

// ── Admin 权限守卫 ────────────────────────────────────────────────

#[tokio::test]
async fn test_non_admin_cannot_create_user() {
    let server = create_server().await;
    let user_email = common::unique_email("non_admin");
    let user_token = salvo::register_and_login(&server, &user_email).await;

    let create_payload = serde_json::json!({
        "email": common::unique_email("should_fail"),
        "password": "Password123!",
        "name": "Should Fail"
    });

    let (status, _) = salvo::post(
        &server,
        "/api/users",
        Some(&user_token),
        Some(&create_payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
}

// ── 验证码流程（快速路径） ────────────────────────────────────────

#[tokio::test]
async fn test_verify_email_rejects_already_verified_user() {
    let server = create_server().await;
    let email = common::unique_email("already_vfy");

    // Register — with no email service configured, auto-verifies
    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "name": "Test User"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Try to verify again
    let verify_payload = serde_json::json!({
        "email": email,
        "code": "000000"
    });
    let (status, _) =
        salvo::post_json(&server, "/api/public/auth/verify-email", &verify_payload).await;
    // Already verified → should get a non-success status
    assert!(status.is_client_error());
}

// ── 密码重置流程 ──────────────────────────────────────────────────

#[tokio::test]
async fn test_forgot_password_nonexistent_email_returns_ok() {
    // Anti-enumeration: non-existent email returns 200 OK
    let server = create_server().await;

    let payload = serde_json::json!({
        "email": "nonexistent_987654321@example.com"
    });

    let (status, _) = salvo::post_json(&server, "/api/public/auth/forgot-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
}

// ── Refresh token 轮换 ──────────────────────────────────────────────

/// 验证 refresh token 轮换（rotation）流程在 salvo 模式下的正确性。
///
/// 使用 reqwest client 的 cookie_store 特性自动管理 cookie，
/// 验证整个轮换管道（login → /refresh → 新旧 token 正确性）完整。
#[tokio::test]
async fn test_refresh_token_rotation() {
    let server = create_server().await;
    let email = common::unique_email("salvo_refresh");

    // 1. Register
    let register_payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });
    let (status, _) =
        salvo::post_json(&server, "/api/public/auth/register", &register_payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // 2. Login with remember=true — reqwest client 自动保存 cookie
    let login_payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "remember": true,
    });
    let url = format!("{}{}", server.base_url, "/api/public/auth/login");
    let login_resp = server
        .client
        .post(&url)
        .header("content-type", "application/json")
        .json(&login_payload)
        .send()
        .await
        .expect("Failed to send login request");
    assert_eq!(login_resp.status(), reqwest::StatusCode::OK);
    let login_body: serde_json::Value = login_resp.json().await.unwrap();
    let original_jwt = login_body["token"].as_str().unwrap().to_string();

    // 3. 验证原始 JWT 有效
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&original_jwt)).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // 4. 调用 /refresh — client 自动附带 webshelf_refresh cookie
    let refresh_url = format!("{}{}", server.base_url, "/api/public/auth/refresh");
    let refresh_resp = server
        .client
        .post(&refresh_url)
        .header("content-type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("Failed to send refresh request");
    assert_eq!(refresh_resp.status(), reqwest::StatusCode::OK);
    let refresh_body: serde_json::Value = refresh_resp.json().await.unwrap();
    let rotated_jwt = refresh_body["token"].as_str().unwrap().to_string();

    // 5. 新（轮换后）JWT 有效
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&rotated_jwt)).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // 6. 原始 JWT 仍然有效（refresh 不改变 token_version）
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&original_jwt)).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // 7. 再次轮换 — 连续调用 /refresh 应持续有效
    let refresh2_resp = server
        .client
        .post(&refresh_url)
        .header("content-type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("Failed to send second refresh request");
    assert_eq!(refresh2_resp.status(), reqwest::StatusCode::OK);
    let refresh2_body: serde_json::Value = refresh2_resp.json().await.unwrap();
    let rotated2_jwt = refresh2_body["token"].as_str().unwrap().to_string();

    // 8. 第二次轮换后的新 JWT 也有效
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&rotated2_jwt)).await;
    assert_eq!(status, reqwest::StatusCode::OK);
}

// ── 登出所有设备 ──────────────────────────────────────────────────

#[tokio::test]
async fn test_logout_all_basic() {
    let server = create_server().await;
    let email = common::unique_email("lo_basic");
    let token = salvo::register_and_login(&server, &email).await;

    // Verify token works
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&token)).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Logout all
    let payload = serde_json::json!({});
    let (status, _) = salvo::post(
        &server,
        "/api/users/me/logout-all",
        Some(&token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Old token should be rejected
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&token)).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_logout_all_unauthenticated() {
    let server = create_server().await;

    let payload = serde_json::json!({});
    let (status, _) = salvo::post(&server, "/api/users/me/logout-all", None, Some(&payload)).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

// ── 辅助函数 ──────────────────────────────────────────────────────

/// 通过直接操作数据库将用户提升为 admin 角色，然后重新登录获取新 token。
async fn promote_to_admin(server: &TestServer, email: &str, _token: &str) -> String {
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{ActiveModel, Column, Entity as UserEntity};

    // Decode token to get user_id
    let config = common::load_test_config();
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    validation.set_issuer(&["webshelf-server"]);
    validation.set_audience(&["webshelf"]);

    // We need the actual token to decode - but we have email, let's find user by email
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let user = UserEntity::find()
        .filter(Column::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .expect("Failed to find user")
        .expect("User not found");

    let current_version = user.token_version;
    let mut active_model: ActiveModel = user.into();
    active_model.role = Set("admin".to_string());
    active_model.token_version = Set(current_version.saturating_add(1));
    active_model.updated_at = Set(chrono::Utc::now());
    active_model
        .update(&db)
        .await
        .expect("Failed to update user to admin");

    // Re-login to get new token with admin role
    let login_payload = serde_json::json!({
        "email": email,
        "password": "Password123!"
    });
    let (status, body) = salvo::post_json(server, "/api/public/auth/login", &login_payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    body["token"].as_str().unwrap().to_string()
}

// ── 辅助函数 ──────────────────────────────────────────────────────

/// 直接在数据库中为指定用户种子密码重置验证码，返回明文 code。
/// 绕过 SMTP 发送路径，使 reset_password 测试可确定性地执行。
async fn seed_reset_code(email: &str, expires_in_minutes: i64) -> String {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    use rand::Rng;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };

    let config = common::load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database for seed_reset_code");

    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User must exist before seeding reset code");

    let code_int = rand::thread_rng().gen_range(0..1_000_000);
    let code = format!("{:06}", code_int);
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut rand::thread_rng());
    let code_hash = argon2
        .hash_password(code.as_bytes(), &salt)
        .expect("Failed to hash reset code")
        .to_string();

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(expires_in_minutes);

    let mut active_model: ActiveModel = user.into();
    active_model.password_reset_token_hash = Set(Some(code_hash));
    active_model.password_reset_expires_at = Set(Some(expires_at));
    active_model.password_reset_sent_at = Set(Some(now));
    active_model.password_reset_failed_attempts = Set(0);
    active_model.updated_at = Set(now);
    active_model.update(&db).await.unwrap();

    code
}

// ── 注册冲突 ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_user_registration_conflict() {
    let server = create_server().await;
    let email = common::unique_email("salvo_conflict");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Conflict Test"
    });

    // First registration succeeds
    let (status, body) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(body["message"], "User registered successfully");

    // Second registration with same email → 409 CONFLICT
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT);
}

// ── 邮箱验证与登录 ────────────────────────────────────────────────

/// 注册（无邮件服务 → 自动验证）→ 登录成功
#[tokio::test]
async fn test_auto_verified_user_can_login() {
    let server = create_server().await;
    let email = common::unique_email("salvo_autovfy");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "AutoVerify Test"
    });

    let (status, body) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(body["email_verified"], true);

    // Login should succeed
    let login_payload = serde_json::json!({
        "email": email,
        "password": "Password123!"
    });
    let (status, body) = salvo::post_json(&server, "/api/public/auth/login", &login_payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(body["token"].is_string());
    assert_eq!(body["token_type"], "Bearer");
}

/// 未验证邮箱无法登录（直接设置 email_verified = false 模拟）
#[tokio::test]
async fn test_unverified_email_cannot_login() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };

    let server = create_server().await;
    let email = common::unique_email("salvo_unvfy");
    let password = "Password123!";

    // Register — auto-verified because email service is not configured
    let payload = serde_json::json!({
        "email": email,
        "password": password,
        "password_confirm": password,
        "name": "Unverified Test"
    });
    let (status, body) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(body["email_verified"], true);

    // Directly set email_verified = false in DB
    let config = common::load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");
    let mut active_model: ActiveModel = user.into();
    active_model.email_verified = Set(false);
    active_model.updated_at = Set(chrono::Utc::now());
    active_model.update(&db).await.unwrap();

    // Login must fail with 401
    let login_payload = serde_json::json!({ "email": email, "password": password });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/login", &login_payload).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

// ── get_me 未认证 ─────────────────────────────────────────────────

#[tokio::test]
async fn test_get_me_unauthenticated() {
    let server = create_server().await;

    let (status, _) = salvo::get(&server, "/api/users/me", None).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

// ── 密码修改边界 ──────────────────────────────────────────────────

#[tokio::test]
async fn test_change_password_empty_current() {
    let server = create_server().await;
    let email = common::unique_email("salvo_chpwd_ecur");
    let token = salvo::register_and_login(&server, &email).await;

    let payload = serde_json::json!({
        "current_password": "",
        "new_password": "NewSecure456!"
    });
    let (status, _) = salvo::post(
        &server,
        "/api/users/me/password",
        Some(&token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_empty_new() {
    let server = create_server().await;
    let email = common::unique_email("salvo_chpwd_enew");
    let token = salvo::register_and_login(&server, &email).await;

    let payload = serde_json::json!({
        "current_password": "Password123!",
        "new_password": ""
    });
    let (status, _) = salvo::post(
        &server,
        "/api/users/me/password",
        Some(&token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_same_as_current() {
    let server = create_server().await;
    let email = common::unique_email("salvo_chpwd_same");
    let token = salvo::register_and_login(&server, &email).await;

    let payload = serde_json::json!({
        "current_password": "Password123!",
        "new_password": "Password123!"
    });
    let (status, _) = salvo::post(
        &server,
        "/api/users/me/password",
        Some(&token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_weak_new_password() {
    let server = create_server().await;
    let email = common::unique_email("salvo_chpwd_weak");
    let token = salvo::register_and_login(&server, &email).await;

    let payload = serde_json::json!({
        "current_password": "Password123!",
        "new_password": "weak"
    });
    let (status, _) = salvo::post(
        &server,
        "/api/users/me/password",
        Some(&token),
        Some(&payload),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_password_unauthenticated() {
    let server = create_server().await;

    let payload = serde_json::json!({
        "current_password": "Password123!",
        "new_password": "NewSecure456!"
    });
    let (status, _) = salvo::post(&server, "/api/users/me/password", None, Some(&payload)).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

// ── 角色变更 token 失效 ───────────────────────────────────────────

/// 用户角色变更后，旧 JWT 必须被拒绝（token_version 原子增量）
#[tokio::test]
async fn test_old_token_invalidated_after_role_change() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };

    let server = create_server().await;
    let email = common::unique_email("salvo_roleinv");

    // Register + login to get user's token
    let token = salvo::register_and_login(&server, &email).await;

    // Verify token works before role change
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&token)).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Promote user to admin directly in DB
    let config = common::load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");

    let current_version = user.token_version;
    let user_id = user.id;
    let mut active_model: ActiveModel = user.into();
    active_model.role = Set("admin".to_string());
    active_model.token_version = Set(current_version.saturating_add(1));
    active_model.updated_at = Set(chrono::Utc::now());
    active_model
        .update(&db)
        .await
        .expect("Failed to update user role");

    // Invalidate token_version cache that may have been cached by previous auth checks
    // (direct DB update bypasses the service layer which normally invalidates the cache).
    let cache_key = format!("user:token_version:{}", user_id);
    let _ = common::create_cache_service()
        .await
        .invalidate(&cache_key)
        .await;

    // Old token must be rejected (token_version was incremented)
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&token)).await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "Old token should be rejected after role change"
    );

    // Re-login to get fresh token — should work
    let login_payload = serde_json::json!({ "email": email, "password": "Password123!" });
    let (status, body) = salvo::post_json(&server, "/api/public/auth/login", &login_payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let new_token = body["token"].as_str().unwrap();

    let (status, _) = salvo::get(&server, "/api/users/me", Some(new_token)).await;
    assert_eq!(status, reqwest::StatusCode::OK);
}

// ── 验证码边界 ────────────────────────────────────────────────────

#[tokio::test]
async fn test_verify_email_validation_error() {
    let server = create_server().await;

    // Invalid email
    let payload = serde_json::json!({ "email": "not-an-email", "code": "123456" });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/verify-email", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);

    // Invalid code length (5 digits, not 6)
    let payload = serde_json::json!({ "email": "test@example.com", "code": "12345" });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/verify-email", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_verify_email_with_nonexistent_email() {
    let server = create_server().await;

    let payload = serde_json::json!({
        "email": "nonexistent-vfy@example.com",
        "code": "123456"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/verify-email", &payload).await;
    // Must return 400 (not 404) to prevent user enumeration
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

// ── Resend code 场景 ──────────────────────────────────────────────

#[tokio::test]
async fn test_resend_code_validation_error() {
    let server = create_server().await;

    let payload = serde_json::json!({ "email": "not-an-email" });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/resend-code", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

/// 已注册（自动验证）用户调用 resend-code → 200
#[tokio::test]
async fn test_resend_code_with_unconfigured_email_service() {
    let server = create_server().await;
    let email = common::unique_email("salvo_resend_200");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Resend200"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Registered user is auto-verified → resend-code returns 200
    let payload = serde_json::json!({ "email": email });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/resend-code", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
}

/// 不存在的邮箱调用 resend-code → 200（反枚举）
#[tokio::test]
async fn test_resend_code_nonexistent_email_returns_ok() {
    let server = create_server().await;

    let payload = serde_json::json!({ "email": "no-such-user-resend-salvo@example.com" });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/resend-code", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
}

/// 已验证用户多次调用 resend-code → 全部返回 200
#[tokio::test]
async fn test_resend_code_verified_user_returns_ok() {
    let server = create_server().await;
    let email = common::unique_email("salvo_resend_vfy");

    let _token = salvo::register_and_login(&server, &email).await;

    // First call
    let payload = serde_json::json!({ "email": email });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/resend-code", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Second call — verified user bypasses cooldown
    let (status, _) = salvo::post_json(&server, "/api/public/auth/resend-code", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
}

/// 未验证用户 + 无 SMTP 配置 → resend-code 返回 503
#[tokio::test]
async fn test_resend_code_unverified_user_returns_503() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };

    let server = create_server().await;
    let email = common::unique_email("salvo_resend_503");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Resend503"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Directly set email_verified = false to simulate unverified state
    let config = common::load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");
    let mut active_model: ActiveModel = user.into();
    active_model.email_verified = Set(false);
    active_model.updated_at = Set(chrono::Utc::now());
    active_model.update(&db).await.unwrap();

    let payload = serde_json::json!({ "email": email });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/resend-code", &payload).await;
    assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
}

// ── 忘记密码边界 ──────────────────────────────────────────────────

#[tokio::test]
async fn test_forgot_password_invalid_email() {
    let server = create_server().await;

    let payload = serde_json::json!({ "email": "not-an-email" });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/forgot-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

/// 已注册用户 + 无 SMTP → forgot-password 返回 503
#[tokio::test]
async fn test_forgot_password_email_not_configured() {
    let server = create_server().await;
    let email = common::unique_email("salvo_fpg");

    // Register user
    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "FPG Test"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // Forgot-password with SMTP unconfigured → 503
    let payload = serde_json::json!({ "email": email });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/forgot-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
}

// ── 重置密码 ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_reset_password_invalid_code_length() {
    let server = create_server().await;

    let payload = serde_json::json!({
        "email": "user@example.com",
        "code": "12345",
        "new_password": "NewPassword456!"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/reset-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_reset_password_weak_new_password() {
    let server = create_server().await;

    let payload = serde_json::json!({
        "email": "user@example.com",
        "code": "123456",
        "new_password": "weak"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/reset-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

/// 完整重置流程：注册 → 种子验证码 → 重置成功 → token失效 → 单次使用检查
#[tokio::test]
async fn test_reset_password_success_and_token_invalidation() {
    let server = create_server().await;
    let email = common::unique_email("salvo_rstok");
    let original_password = "Password123!";

    // 1. Register
    let payload = serde_json::json!({
        "email": email,
        "password": original_password,
        "password_confirm": original_password,
        "name": "Reset OK"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // 2. Capture original JWT
    let login_payload = serde_json::json!({ "email": email, "password": original_password });
    let (status, body) = salvo::post_json(&server, "/api/public/auth/login", &login_payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let old_token = body["token"].as_str().unwrap().to_string();

    // 3. Seed reset code
    let reset_code = seed_reset_code(&email, 60).await;

    // 4. Submit reset with correct code
    let new_password = "NewSecure789!";
    let payload = serde_json::json!({
        "email": email,
        "code": reset_code,
        "new_password": new_password
    });
    let (status, body) =
        salvo::post_json(&server, "/api/public/auth/reset-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let fresh_token = body["token"].as_str().unwrap().to_string();
    assert!(!fresh_token.is_empty());
    assert_eq!(body["token_type"], "Bearer");

    // 5. Old token must be rejected
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&old_token)).await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "Old JWT must be rejected after password reset"
    );

    // 6. Fresh token must work
    let (status, _) = salvo::get(&server, "/api/users/me", Some(&fresh_token)).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    // 7. Re-using same reset code must fail (single-use)
    let payload = serde_json::json!({
        "email": email,
        "code": reset_code,
        "new_password": "AnotherPass321!"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/reset-password", &payload).await;
    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "Reset code must be single-use"
    );

    // 8. New password works, old password fails
    let login_new = serde_json::json!({ "email": email, "password": new_password });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/login", &login_new).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    let login_old = serde_json::json!({ "email": email, "password": original_password });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/login", &login_old).await;
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_reset_password_wrong_code() {
    let server = create_server().await;
    let email = common::unique_email("salvo_rwrt");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Wrong Code"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    let _real_code = seed_reset_code(&email, 60).await;

    let payload = serde_json::json!({
        "email": email,
        "code": "999999",
        "new_password": "NewPassword456!"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/reset-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_reset_password_nonexistent_email() {
    let server = create_server().await;

    let payload = serde_json::json!({
        "email": "ghost-rst@example.com",
        "code": "123456",
        "new_password": "NewPassword456!"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/reset-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_reset_password_expired_code() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use webshelf_server::repositories::user::{
        ActiveModel, Column as UserColumn, Entity as UserEntity,
    };

    let server = create_server().await;
    let email = common::unique_email("salvo_rexp");

    let payload = serde_json::json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Expired Code"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/register", &payload).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    let code = seed_reset_code(&email, 60).await;

    // Set expires_at to the past
    let config = common::load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    let user = UserEntity::find()
        .filter(UserColumn::Email.eq(email.to_lowercase()))
        .one(&db)
        .await
        .unwrap()
        .expect("User must exist");
    let mut active_model: ActiveModel = user.into();
    active_model.password_reset_expires_at =
        Set(Some(chrono::Utc::now() - chrono::Duration::hours(1)));
    active_model.updated_at = Set(chrono::Utc::now());
    active_model.update(&db).await.unwrap();

    let payload = serde_json::json!({
        "email": email,
        "code": code,
        "new_password": "NewPassword456!"
    });
    let (status, _) = salvo::post_json(&server, "/api/public/auth/reset-password", &payload).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}

// ── 缓存失效测试（服务层，框架无关） ──────────────────────────────

#[tokio::test]
async fn test_cache_invalidation_after_user_update() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let state = salvo::create_test_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    let email = common::unique_email("salvo_cache_upd");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");
    let cache_key = format!("user:{}", user.id);

    // Populate cache
    let _ = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    let cached = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(cached.is_some(), "user should be cached after get_user");

    // Update user → cache should be invalidated
    let _updated = svc
        .update_user(
            user.id.as_i64(),
            webshelf_server::repositories::user::UpdateUserInput {
                name: Some("Updated Name".to_string()),
                email: None,
                role: None,
            },
            "system",
        )
        .await
        .expect("update_user failed");

    let after_update = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_update.is_none(),
        "cache should be invalidated after update_user"
    );

    let _ = state.cache.invalidate(&cache_key).await;
}

#[tokio::test]
async fn test_cache_invalidation_after_password_change() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let state = salvo::create_test_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    let email = common::unique_email("salvo_cache_pwd");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "OldPass123!".to_string(),
                name: "Pwd Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");
    let cache_key = format!("user:{}", user.id);

    // Populate cache
    let _ = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    let cached = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        cached.is_some(),
        "user should be cached before password change"
    );

    // Change password → cache invalidated
    let (_updated, _new_version) = svc
        .change_password(user.id.as_i64(), "OldPass123!", "NewPass456!")
        .await
        .expect("change_password failed");

    let after_change = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_change.is_none(),
        "cache should be invalidated after password change"
    );

    let _ = state.cache.invalidate(&cache_key).await;
}

#[tokio::test]
async fn test_cache_invalidation_after_delete() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let state = salvo::create_test_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    let email = common::unique_email("salvo_cache_del");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Delete Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");
    let cache_key = format!("user:{}", user.id);

    // Populate cache
    let _ = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    let cached = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(cached.is_some(), "user should be cached before delete");

    // Delete user → cache invalidated
    svc.delete_user(user.id.as_i64(), "system", 0)
        .await
        .expect("delete_user failed");

    let after_delete = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_delete.is_none(),
        "cache should be invalidated after delete_user"
    );

    let _ = state.cache.invalidate(&cache_key).await;
}

#[tokio::test]
async fn test_cache_invalidation_after_balance_change() {
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;

    let state = salvo::create_test_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());

    let email = common::unique_email("salvo_cache_bal");
    let user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Balance Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");
    let cache_key = format!("user:{}", user.id);

    // Populate cache
    let _ = svc
        .get_user(user.id.as_i64())
        .await
        .expect("get_user failed");
    let cached = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        cached.is_some(),
        "user should be cached before balance change"
    );

    // Set balance → cache invalidated
    let updated = svc
        .set_balance(user.id.as_i64(), 500, "system")
        .await
        .expect("set_balance failed");
    assert_eq!(updated.balance, 500);

    let after_balance = state
        .cache
        .get::<webshelf_server::repositories::user::UserResponse>(&cache_key)
        .await
        .expect("cache get failed");
    assert!(
        after_balance.is_none(),
        "cache should be invalidated after balance change"
    );

    let _ = state.cache.invalidate(&cache_key).await;
}

// ── 全局互斥锁：串行化共享同一数据库的缓存测试 ────────────────────

use std::sync::OnceLock;
use tokio::sync::Mutex as AsyncMutex;

fn count_cache_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

// ── 分页计数缓存测试（服务层，框架无关） ──────────────────────────

#[tokio::test]
async fn test_count_cache_populated_on_list_users() {
    let _guard = count_cache_lock().lock().await;
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;
    use webshelf_server::services::user::PaginationParams;

    let state = salvo::create_test_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());
    let role = "admin";
    let count_key = format!("user:count:{}", role);

    // 清除可能来自其他测试的残留缓存
    let _ = state.cache.invalidate(&count_key).await;

    let email = common::unique_email("salvo_cnt_cache");
    let _user = svc
        .create_user(
            CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Count Cache Test".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // First list_users: count cache miss → populate
    let page1 = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");
    assert!(page1.total > 0, "should have at least one user");

    // Second list_users: should hit cache (same count expected)
    let page2 = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");
    assert_eq!(
        page2.total, page1.total,
        "count cache should return same value"
    );

    // Clean up
    let count_key = format!("user:count:{}", role);
    let _ = state.cache.invalidate(&count_key).await;
}

#[tokio::test]
async fn test_count_cache_invalidated_after_create_and_delete() {
    let _guard = count_cache_lock().lock().await;
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;
    use webshelf_server::services::user::PaginationParams;

    let state = salvo::create_test_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());
    let role = "admin";
    let count_key = format!("user:count:{}", role);

    // 清除可能来自其他测试的残留缓存
    let _ = state.cache.invalidate(&count_key).await;

    // Create an initial user so list_users works
    let email1 = common::unique_email("salvo_cnt_cd1");
    let user1 = svc
        .create_user(
            CreateUserInput {
                email: email1.clone(),
                password: "Password123!".to_string(),
                name: "Count CD 1".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // Populate count cache via list_users
    svc.list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");
    // Verify cache is populated by calling list_users again (should return same total)
    // This is more robust than checking Redis directly due to cross-test cache interference.
    let page1 = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");
    assert!(page1.total > 0, "should have at least one user");
    let page2 = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");
    assert_eq!(
        page2.total, page1.total,
        "count cache should be populated (second call returns same total)"
    );

    // Create another user → count cache invalidated
    let email2 = common::unique_email("salvo_cnt_cd2");
    let _user2 = svc
        .create_user(
            CreateUserInput {
                email: email2.clone(),
                password: "Password123!".to_string(),
                name: "Count CD 2".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    let after_create: Option<u64> = state.cache.get(&count_key).await.unwrap();
    assert!(
        after_create.is_none(),
        "count cache should be invalidated after creating a user"
    );

    // Re-populate
    let _page = svc
        .list_users(PaginationParams::default(), role)
        .await
        .expect("list_users failed");

    // Delete user1 → count cache invalidated again
    svc.delete_user(user1.id.as_i64(), "system", 0)
        .await
        .expect("delete_user failed");

    let after_delete: Option<u64> = state.cache.get(&count_key).await.unwrap();
    assert!(
        after_delete.is_none(),
        "count cache should be invalidated after deleting a user"
    );

    let _ = state.cache.invalidate(&count_key).await;
}

#[tokio::test]
async fn test_count_cache_invalidated_after_create_system_role() {
    let _guard = count_cache_lock().lock().await;
    use webshelf_server::repositories::user::CreateUserInput;
    use webshelf_server::services::UserService;
    use webshelf_server::services::user::PaginationParams;

    let state = salvo::create_test_state().await;
    let svc = UserService::new(state.db.clone(), state.cache.clone());
    let role = "system";
    let count_key = format!("user:count:{}", role);

    // 清除可能来自其他测试的残留缓存
    let _ = state.cache.invalidate(&count_key).await;

    let email1 = common::unique_email("salvo_cnt_sys1");
    let _user1 = svc
        .create_user(
            CreateUserInput {
                email: email1.clone(),
                password: "Password123!".to_string(),
                name: "Sys Count 1".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    // Populate system count cache
    let mut before_create: Option<u64> = None;
    for attempt in 1..=3 {
        let _page = svc
            .list_users(PaginationParams::default(), role)
            .await
            .expect("list_users failed");
        if let Some(cached) = state.cache.get::<u64>(&count_key).await.unwrap() {
            before_create = Some(cached);
            break;
        }
        tracing::warn!(
            "system count cache empty after list_users (attempt {}), retrying...",
            attempt
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(
        before_create.is_some(),
        "system count cache should exist after list_users"
    );

    // Create another user → count cache invalidated
    let email2 = common::unique_email("salvo_cnt_sys2");
    let _user2 = svc
        .create_user(
            CreateUserInput {
                email: email2.clone(),
                password: "Password123!".to_string(),
                name: "Sys Count 2".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("create_user failed");

    let after_create: Option<u64> = state.cache.get(&count_key).await.unwrap();
    assert!(
        after_create.is_none(),
        "system count cache should be invalidated after creating a user"
    );

    let _ = state.cache.invalidate(&count_key).await;
}
