#![cfg(not(feature = "webshelf-salvo"))]

//! Integration tests for `POST /api/users/me/logout-all`.
//!
//! Tests cover the session invalidation mechanism:
//! 1. Old JWT is rejected (401) after successful logout-all
//! 2. Refresh-token renewal is rejected after logout-all
//! 3. Concurrent logout-all calls are idempotent
//! 4. Unauthenticated requests are rejected (401)
//! 5. The current session request itself succeeds (returns 200)
//!
//! NOTE: These tests require running PostgreSQL and Redis instances.
//! Run with: cargo test --test logout_all_test

use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;
use webshelf_axum::middleware::auth_middleware;
use webshelf_axum::{Any, Body, CorsLayer, Method, Request, Router, StatusCode, TraceLayer};
use webshelf_axum::{BodyExt, from_fn, from_fn_with_state};

/// Refresh cookie name.
const REFRESH_COOKIE: &str = "webshelf_refresh";

// ── Helpers ────────────────────────────────────────────────────────────────

/// Create a full test application with the same middleware stack as production.
async fn create_test_app() -> Router {
    use distributed_ratelimit::{RateLimitConfig, RedisRateLimiter};
    use sea_orm::Database;
    use webshelf_server::AutoRouter;
    use webshelf_server::services::CacheService;
    use webshelf_server::utils::load_config;
    use webshelf_server::{
        AppState,
        routes::{api_routes, auth_routes},
    };

    let config = load_config("config.toml", "development").expect("Failed to load config");

    let db = Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    let db = AutoRouter::single(db);
    webshelf_server::migrations::run_migrations(db.write_conn())
        .await
        .expect("Failed to run migrations");

    webshelf_server::snowflake::init(db.write_conn())
        .await
        .expect("Failed to initialize Snowflake generator");

    let cache = CacheService::new(&config.redis_url, config.cache_max_connections).await;

    let state = AppState {
        db,
        cache,
        config: Arc::new(config),
        email: emailserver::EmailService::new(emailserver::EmailConfig::default()),
        wechat: None,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    Router::new()
        .nest(
            "/api",
            api_routes().layer(from_fn_with_state(
                state.clone(),
                auth_middleware::<AppState>,
            )),
        )
        .nest(
            "/api/public/auth",
            auth_routes(RedisRateLimiter::disabled(RateLimitConfig::default())),
        )
        .layer(from_fn(webshelf_axum::middleware::panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// Generate a unique test email using a nanosecond timestamp.
fn unique_email(label: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}_{}@example.com", label, ts)
}

/// Register a new user and log in. Returns the JWT token from the login
/// response body.
async fn register_and_login(app: &Router, email: &str) -> String {
    let register_payload = json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&register_payload).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    login(app, email, "Password123!", false).await
}

/// Log in and return the JWT token from the response body.
async fn login(app: &Router, email: &str, password: &str, remember: bool) -> String {
    let login_payload = json!({
        "email": email,
        "password": password,
        "remember": remember,
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    body["token"].as_str().unwrap().to_string()
}

/// Register a new user, then log in with `remember=true` and extract both the
/// JWT and the refresh-token cookie from the response. Returns `(jwt, refresh_token)`.
async fn register_and_login_with_refresh(app: &Router, email: &str) -> (String, String) {
    // 1. Register user first
    let register_payload = json!({
        "email": email,
        "password": "Password123!",
        "password_confirm": "Password123!",
        "name": "Test User"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&register_payload).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Login with remember=true to get JWT + refresh token
    let login_payload = json!({
        "email": email,
        "password": "Password123!",
        "remember": true,
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&login_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Extract refresh token from Set-Cookie headers.
    let refresh_token = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .filter_map(|cookie_str| {
            // Parse "webshelf_refresh=<value>; ..."
            if let Some(value_start) = cookie_str.find(REFRESH_COOKIE) {
                let after_name = &cookie_str[value_start + REFRESH_COOKIE.len()..];
                if after_name.starts_with('=') {
                    let value_end = after_name.find(';').unwrap_or(after_name.len());
                    Some(after_name[1..value_end].to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .next()
        .expect("Set-Cookie for webshelf_refresh not found");

    // Parse the JSON body for the JWT.
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let jwt = body["token"].as_str().unwrap().to_string();

    (jwt, refresh_token)
}

/// Build a Cookie header value string for the refresh token.
fn refresh_cookie_header(token: &str) -> String {
    format!("{}={}", REFRESH_COOKIE, token)
}

/// Call `POST /api/users/me/logout-all` with the given bearer token.
async fn call_logout_all(app: &Router, token: &str) -> StatusCode {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/logout-all")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

/// Call a protected endpoint (`GET /api/users/me`) to verify whether a JWT
/// is still accepted.
async fn check_token_valid(app: &Router, token: &str) -> StatusCode {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/me")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

/// Call the refresh endpoint with a given refresh-token cookie value.
async fn call_refresh(app: &Router, refresh_token: &str) -> StatusCode {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/refresh")
                .header("cookie", refresh_cookie_header(refresh_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

// ── Tests ──────────────────────────────────────────────────────────────────

/// (1) 正常调用后旧 JWT 被拒绝返回 401。
///
/// 验证 token_version 递增机制：logout-all 之后，之前签发的 JWT
/// 应被认证中间件拒绝。
#[tokio::test]
async fn test_old_jwt_rejected_after_logout_all() {
    let app = create_test_app().await;
    let email = unique_email("old_jwt_rejected");

    // 注册并登录，获取旧 JWT
    let old_token = register_and_login(&app, &email).await;

    // 验证旧 JWT 当前可用
    assert_eq!(
        check_token_valid(&app, &old_token).await,
        StatusCode::OK,
        "Old JWT should be valid before logout-all"
    );

    // 调用登出所有设备
    let status = call_logout_all(&app, &old_token).await;
    assert_eq!(status, StatusCode::OK, "logout-all should succeed");

    // 旧 JWT 应该被拒绝
    assert_eq!(
        check_token_valid(&app, &old_token).await,
        StatusCode::UNAUTHORIZED,
        "Old JWT should be rejected after logout-all (token_version mismatch)"
    );

    // 重新登录应获得新 JWT，且新 JWT 可用
    let new_token = login(&app, &email, "Password123!", false).await;
    assert_eq!(
        check_token_valid(&app, &new_token).await,
        StatusCode::OK,
        "New JWT obtained after logout-all should be valid"
    );
}

/// (2) 调用后 refresh token 续期被拒绝。
///
/// 验证 `revoke_all_sessions` 删除了所有 refresh token，
/// 尝试使用旧 refresh token 调用 `/api/public/auth/refresh` 应返回 401。
#[tokio::test]
async fn test_refresh_token_rejected_after_logout_all() {
    let app = create_test_app().await;
    let email = unique_email("refresh_rejected");

    // 注册并用 remember=true 登录以获得 JWT + refresh token
    let (jwt, refresh_token) = register_and_login_with_refresh(&app, &email).await;

    // 先验证 refresh token 在 logout-all 之前是可以续期的
    assert_eq!(
        call_refresh(&app, &refresh_token).await,
        StatusCode::OK,
        "Refresh token should be valid before logout-all"
    );

    // 调用登出所有设备
    let status = call_logout_all(&app, &jwt).await;
    assert_eq!(status, StatusCode::OK, "logout-all should succeed");

    // 同一 refresh token 续期应被拒绝（refresh token 已被从 DB 删除）
    assert_eq!(
        call_refresh(&app, &refresh_token).await,
        StatusCode::UNAUTHORIZED,
        "Refresh token should be rejected after logout-all (deleted from DB)"
    );
}

/// (3) 同一用户并发调用保持幂等性。
///
/// 多个并发任务同时调用 logout-all：第一个任务成功（200），
/// 其余任务因 JWT 已被第一个任务递增 token_version 而失效（401）。
/// 这是 token_version 机制的正确行为，幂等性体现在服务端状态
/// 不会因并发调用而损坏：最终所有旧 JWT 均被拒绝。
#[tokio::test]
async fn test_concurrent_logout_all_idempotent() {
    let app = create_test_app().await;
    let email = unique_email("concurrent");

    let old_token = register_and_login(&app, &email).await;

    // 创建 5 个并发任务，同时调用 logout-all
    let mut handles = Vec::new();
    for i in 0..5 {
        let app = app.clone();
        let token = old_token.clone();
        handles.push(tokio::spawn(async move {
            let status = call_logout_all(&app, &token).await;
            (i, status)
        }));
    }

    // 至少有一个任务返回 200（最先执行的那个），其余可能返回 401
    // （因为它们的 JWT 已被最先成功的那次调用的 token_version 递增而失效）。
    // 所有任务都不能返回 500（无服务端崩溃）。
    let mut has_ok = false;
    for handle in handles {
        let (i, status) = handle.await.unwrap();
        assert_ne!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "Concurrent logout-all task {} returned 500",
            i
        );
        if status == StatusCode::OK {
            has_ok = true;
        }
    }
    assert!(
        has_ok,
        "At least one concurrent logout-all call should succeed"
    );

    // 旧 JWT 应被拒绝（无论是哪个任务使 token_version 递增）
    assert_eq!(
        check_token_valid(&app, &old_token).await,
        StatusCode::UNAUTHORIZED,
        "Old JWT should be rejected after concurrent logout-all"
    );
}

/// (4) 未认证用户返回 401。
///
/// 不带 Authorization 头或无效 token 应被认证中间件拦截。
#[tokio::test]
async fn test_logout_all_unauthenticated() {
    let app = create_test_app().await;

    // 不带认证头
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/logout-all")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 带无效的 Bearer token
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/logout-all")
                .header("content-type", "application/json")
                .header("authorization", "Bearer invalid-token-that-is-not-a-jwt")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// (5) 当前会话的请求本身成功返回 200。
///
/// 即使 logout-all 会使后续请求失效，发起登出的当前请求
/// 仍应成功返回 200，且响应体包含正确的 message。
#[tokio::test]
async fn test_logout_all_current_session_succeeds() {
    let app = create_test_app().await;
    let email = unique_email("self_not_logged_out");

    let token = register_and_login(&app, &email).await;

    // 发送请求并检查响应状态，同时保留 body 用于后续断言
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users/me/logout-all")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // 当前请求应成功返回 200
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "The current session's logout-all request itself should return 200"
    );

    // 验证响应体包含预期的 message
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["message"], "Logged out from all devices");
}
