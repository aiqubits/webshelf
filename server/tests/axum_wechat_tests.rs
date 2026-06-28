#![cfg(not(feature = "webshelf-salvo"))]

//! Integration tests for the WeChat captcha-login feature.
//!
//! Covers:
//! - `GET /api/public/auth/wechat-enabled` — feature toggle endpoint
//! - `POST /api/public/auth/wx-login` — captcha login (with and without Redis)
//! - `GET /api/public/wechat/callback` — WeChat server verification handshake
//! - `POST /api/public/wechat/callback` — WeChat message callback processing
//!
//! Some tests require running Redis and PostgreSQL instances.
//! Tests that require Redis are marked `#[ignore]` and can be run with:
//!   cargo test --test axum_wechat_tests -- --ignored

use std::sync::Arc;

use tower::ServiceExt;
use webshelf_axum::{Body, Method, Request, Router, StatusCode};
use webshelf_axum::{BodyExt, from_fn, from_fn_with_state};
use webshelf_server::middlewares::auth_middleware;
use wechat_api::{CaptchaStore, UserBindingStore};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Generate a unique test email using a nanosecond timestamp.
fn unique_email(label: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}_{}@example.com", label, ts)
}

/// Load the test configuration.
fn load_test_config() -> webshelf_server::utils::AppConfig {
    webshelf_server::utils::load_config("config.toml", "development")
        .expect("Failed to load config.toml for tests")
}

/// Create a test database with migrations.
async fn create_test_db() -> Arc<webshelf_server::AutoRouter> {
    let config = load_test_config();
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");
    let db = webshelf_server::AutoRouter::single(db);
    webshelf_server::migrations::run_migrations(db.write_conn())
        .await
        .expect("Failed to run migrations");
    webshelf_server::snowflake::init(db.write_conn())
        .await
        .expect("Failed to initialize Snowflake generator");
    db
}

/// Create a cache service from config.
async fn create_cache() -> webshelf_server::services::CacheService {
    let config = load_test_config();
    webshelf_server::services::CacheService::new(&config.redis_url, config.cache_max_connections)
        .await
}

/// Build an `AppState` with WeChat components enabled and configurable captcha
/// length. Used by tests that need to verify captcha code length behaviour.
async fn create_wechat_state_with_len(
    account_id: &str,
    captcha_len: usize,
) -> webshelf_server::AppState {
    let config = load_test_config();
    let db = create_test_db().await;
    let cache = create_cache().await;

    // Mutate config to enable WeChat with test credentials.
    let mut wechat_cfg = config.wechat.clone();
    wechat_cfg.enabled = true;
    wechat_cfg.account_id = account_id.to_string();
    wechat_cfg.app_id = "test_app_id".to_string();
    wechat_cfg.app_secret = "test_app_secret".to_string();
    wechat_cfg.token = "test_token".to_string();
    wechat_cfg.captcha_len = captcha_len;

    let mut app_config = config;
    app_config.wechat = wechat_cfg;

    let wechat = webshelf_server::services::wechat::init_wechat_components(
        &app_config.wechat,
        &cache,
        db.clone(),
    );

    webshelf_server::AppState {
        db,
        cache,
        config: Arc::new(app_config),
        email: emailserver::EmailService::new(emailserver::EmailConfig::default()),
        wechat,
    }
}

/// Build an `AppState` with WeChat components enabled using test credentials.
///
/// The WeChat captcha-login feature requires valid-looking credentials even
/// though no real WeChat server is involved. The `account_id` distinguishes
/// test accounts to avoid key collisions when tests run in parallel.
async fn create_wechat_state(account_id: &str) -> webshelf_server::AppState {
    create_wechat_state_with_len(account_id, 5).await
}

/// Build a test router with the same middleware stack as production,
/// but using the provided (WeChat-enabled) state.
fn build_router(state: webshelf_server::AppState) -> Router {
    use distributed_ratelimit::{RateLimitConfig, RedisRateLimiter};
    use webshelf_axum::{CorsLayer, TraceLayer};

    let cors = CorsLayer::new()
        .allow_origin(webshelf_axum::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(webshelf_axum::Any);

    // Use the production build_app_router, but we replicate the key parts here
    // so the test is self-contained. The critical thing is that the WeChat
    // callback routes are conditionally registered based on state.wechat.
    let router = Router::new()
        .nest(
            "/api",
            webshelf_server::routes::api_routes().layer(from_fn_with_state(
                state.clone(),
                auth_middleware::<webshelf_server::AppState>,
            )),
        )
        .nest(
            "/api/public/auth",
            webshelf_server::routes::auth_routes(RedisRateLimiter::disabled(
                RateLimitConfig::default(),
            )),
        )
        .merge(if state.wechat.is_some() {
            Router::new().route(
                "/api/public/wechat/callback",
                webshelf_axum::get(webshelf_server::handlers::wechat::wechat_callback_get).merge(
                    webshelf_axum::post(webshelf_server::handlers::wechat::wechat_callback_post),
                ),
            )
        } else {
            Router::new()
        })
        .layer(from_fn(webshelf_server::middlewares::panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    router
}

/// Build a simple test router WITHOUT WeChat components (wechat: None).
async fn build_router_without_wechat() -> Router {
    use distributed_ratelimit::{RateLimitConfig, RedisRateLimiter};
    use webshelf_axum::{CorsLayer, TraceLayer};

    let config = load_test_config();
    let db = create_test_db().await;
    let cache = create_cache().await;

    let state = webshelf_server::AppState {
        db,
        cache,
        config: Arc::new(config),
        email: emailserver::EmailService::new(emailserver::EmailConfig::default()),
        wechat: None,
    };

    let cors = CorsLayer::new()
        .allow_origin(webshelf_axum::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(webshelf_axum::Any);

    Router::new()
        .nest(
            "/api",
            webshelf_server::routes::api_routes().layer(from_fn_with_state(
                state.clone(),
                auth_middleware::<webshelf_server::AppState>,
            )),
        )
        .nest(
            "/api/public/auth",
            webshelf_server::routes::auth_routes(RedisRateLimiter::disabled(
                RateLimitConfig::default(),
            )),
        )
        .layer(from_fn(webshelf_server::middlewares::panic_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// Parse the response body as JSON.
async fn body_to_json(response: webshelf_axum::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Parse the response body as UTF-8 string (for XML / text).
async fn body_text(response: webshelf_axum::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

/// Compute the WeChat signature for test callback parameters.
fn compute_signature(token: &str, timestamp: &str, nonce: &str) -> String {
    wechat_api::crypto::compute_signature(token, timestamp, nonce)
}

/// Extract the captcha code from a WeChat callback XML reply.
///
/// The reply XML has the format:
/// `<Content><![CDATA[Your verification code: ABCDE\nEnter this code...]]></Content>`
fn extract_code_from_reply(xml: &str) -> Option<&str> {
    let prefix = "verification code: ";
    let start = xml.find(prefix)? + prefix.len();
    let after = &xml[start..];
    let end = after
        .find(|ch: char| ch.is_whitespace() || ch == '<')
        .unwrap_or(after.len());
    Some(&after[..end])
}

// ── Tests for unconfigured WeChat ────────────────────────────────────────────
//
// These tests verify the behaviour when `state.wechat` is `None` (the default).

#[tokio::test]
async fn test_wechat_enabled_false_when_not_configured() {
    let app = build_router_without_wechat().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/public/auth/wechat-enabled")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response).await;
    assert_eq!(body["enabled"], false);
}

#[tokio::test]
async fn test_wx_login_wechat_not_configured() {
    let app = build_router_without_wechat().await;

    let payload = serde_json::json!({ "code": "ABC12" });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_to_json(response).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("not configured"),
        "expected 'not configured' message, got: {:?}",
        body
    );
}

// ── Tests for configured WeChat (no Redis required) ─────────────────────────
//
// These tests use `build_router` with WeChat components enabled.
// The RedisCaptchaStore uses a graceful no-op mode when Redis is unavailable,
// so all store operations return None / no-op without error.

#[tokio::test]
async fn test_wechat_enabled_true_when_configured() {
    let state = create_wechat_state("test_enabled_true").await;
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/public/auth/wechat-enabled")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_json(response).await;
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn test_wx_login_invalid_captcha() {
    // Without Redis, the captcha store returns None for any key,
    // so a request with a non-existent code gets a 400 "not found" error.
    let state = create_wechat_state("test_invalid_captcha").await;
    let app = build_router(state);

    let payload = serde_json::json!({ "code": "NONEXIST" });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_to_json(response).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid or expired"),
        "expected captcha error, got: {:?}",
        body
    );
}

#[tokio::test]
async fn test_callback_get_valid_signature() {
    let state = create_wechat_state("test_cb_get_valid").await;
    let app = build_router(state);
    let token = "test_token";
    let ts = "1609459200";
    let nonce = "nonce123";
    let sig = compute_signature(token, ts, nonce);

    let uri = format!(
        "/api/public/wechat/callback?signature={}&timestamp={}&nonce={}&echostr=HELLO_ECHO",
        sig, ts, nonce
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "valid signature should return 200"
    );
    let text = body_text(response).await;
    assert_eq!(
        text, "HELLO_ECHO",
        "response body must be the echostr value"
    );
}

#[tokio::test]
async fn test_callback_get_invalid_signature() {
    let state = create_wechat_state("test_cb_get_invalid").await;
    let app = build_router(state);

    let uri = "/api/public/wechat/callback?signature=BAD&timestamp=1&nonce=n&echostr=ECHO";

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "invalid signature should return 400"
    );
}

#[tokio::test]
async fn test_callback_post_trigger_keyword() {
    let state = create_wechat_state("test_cb_post_trig").await;
    let app = build_router(state);
    let token = "test_token";
    let ts = "1609459200";
    let nonce = "nonce456";
    let sig = compute_signature(token, ts, nonce);

    let uri = format!(
        "/api/public/wechat/callback?signature={}&timestamp={}&nonce={}",
        sig, ts, nonce
    );

    // XML body with a trigger keyword (验证码).
    let body = r#"<xml><ToUserName><![CDATA[gh_test]]></ToUserName><FromUserName><![CDATA[oTriggerUser]]></FromUserName><CreateTime>1609459200</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[验证码]]></Content><MsgId>1234567890</MsgId></xml>"#;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header("content-type", "application/xml")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "callback with trigger keyword should return 200"
    );

    // Response must be XML containing a verification code.
    let text = body_text(response).await;
    assert!(
        text.contains("<Content><![CDATA["),
        "response must contain a text reply XML, got: {}",
        text
    );
    assert!(
        text.contains("verification code") || text.contains("验证码"),
        "response must mention captcha code, got: {}",
        text
    );
}

#[tokio::test]
async fn test_callback_post_non_keyword() {
    let state = create_wechat_state("test_cb_post_help").await;
    let app = build_router(state);
    let token = "test_token";
    let ts = "1609459200";
    let nonce = "nonce789";
    let sig = compute_signature(token, ts, nonce);

    let uri = format!(
        "/api/public/wechat/callback?signature={}&timestamp={}&nonce={}",
        sig, ts, nonce
    );

    // XML body with a non-trigger text message.
    let body = r#"<xml><ToUserName><![CDATA[gh_test]]></ToUserName><FromUserName><![CDATA[oHelpUser]]></FromUserName><CreateTime>1609459200</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[hello world]]></Content><MsgId>1234567891</MsgId></xml>"#;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header("content-type", "application/xml")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "callback with non-keyword should return 200"
    );

    let text = body_text(response).await;
    assert!(
        text.contains("Send") || text.contains("发送"),
        "response must contain a help message, got: {}",
        text
    );
}

#[tokio::test]
async fn test_callback_post_malformed_xml() {
    let state = create_wechat_state("test_cb_post_badxml").await;
    let app = build_router(state);
    let token = "test_token";
    let ts = "1609459200";
    let nonce = "nonce000";
    let sig = compute_signature(token, ts, nonce);

    let uri = format!(
        "/api/public/wechat/callback?signature={}&timestamp={}&nonce={}",
        sig, ts, nonce
    );

    // Malformed XML body.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header("content-type", "application/xml")
                .body(Body::from("not xml at all".to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "malformed XML should still return 200 (handler absorbs errors to suppress WeChat retries)"
    );

    // The handler always returns 200 with either the reply XML or "success".
    let text = body_text(response).await;
    assert_eq!(
        text, "success",
        "malformed XML should produce 'success' fallback body"
    );
}

// ── Tests requiring Redis (ignored by default) ─────────────────────────────
//
// These tests seed captcha data directly into Redis and then verify the
// full wx-login HTTP flow.  Requires a running Redis on 127.0.0.1:6379.
//
// Run with: cargo test --test axum_wechat_tests -- --ignored

/// Helper to seed a captcha directly into the RedisCaptchaStore for tests.
async fn seed_captcha(
    state: &webshelf_server::AppState,
    account_id: &str,
    openid: &str,
    code: &str,
) {
    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    let key = wechat.captcha_service.captcha_key(account_id, openid);
    wechat
        .captcha_store
        .set_captcha(&key, code, openid, std::time::Duration::from_secs(60))
        .await
        .expect("Failed to seed captcha in Redis");
}

#[tokio::test]
#[ignore]
async fn test_wx_login_success() {
    let account_id = "test_wx_login_ok";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());
    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    // 1. Create a user in the database.
    let email = unique_email("wx_ok");
    let user = webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
        .create_user(
            webshelf_server::repositories::user::CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "WxLogin Ok".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user for wx-login test");

    // 2. Bind the openid to the user.
    let openid = "oWxLoginOk";
    wechat
        .login_service
        .bindings
        .bind_openid(&user.id.as_i64(), openid)
        .await
        .expect("Failed to bind openid");

    // 3. Seed a captcha in Redis.
    let code = "HELLO";
    seed_captcha(&state, account_id, openid, code).await;

    // 4. Call wx-login with the captcha code.
    let payload = serde_json::json!({ "code": code });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "wx-login with valid captcha should succeed"
    );

    let body = body_to_json(response).await;
    assert!(
        body["token"].as_str().unwrap_or("").len() > 0,
        "JWT must be issued"
    );
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["role"], "user");
    assert!(
        body["user_id"]
            .as_str()
            .unwrap_or("")
            .contains(&user.id.to_string()),
        "JWT must be for the bound user"
    );
}

#[tokio::test]
#[ignore]
async fn test_wx_login_unbound_openid() {
    let account_id = "test_wx_login_unbound";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());

    // Seed a captcha in Redis without binding any user to the openid.
    let openid = "oUnboundUser";
    let code = "UNBND";
    seed_captcha(&state, account_id, openid, code).await;

    // Call wx-login — should fail with UserNotBound error.
    let payload = serde_json::json!({ "code": code });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "unbound openid should return 400"
    );

    let body = body_to_json(response).await;
    let msg = body["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("Invalid or expired"),
        "unbound openid must return the same generic message as invalid captcha, got: {}",
        msg
    );
}

#[tokio::test]
#[ignore]
async fn test_wx_login_wrong_code() {
    let account_id = "test_wx_login_wrong";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());
    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    // Create a user and bind openid.
    let email = unique_email("wx_wrong");
    let user = webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
        .create_user(
            webshelf_server::repositories::user::CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Wx Wrong Code".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user");

    let openid = "oWxWrongCode";
    wechat
        .login_service
        .bindings
        .bind_openid(&user.id.as_i64(), openid)
        .await
        .expect("Failed to bind openid");

    // Seed captcha with one code, but submit a different one.
    seed_captcha(&state, account_id, openid, "RIGHT").await;

    let payload = serde_json::json!({ "code": "WRONG" });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "wrong captcha code should return 400"
    );
}

#[tokio::test]
#[ignore]
async fn test_wx_login_expired_captcha() {
    let account_id = "test_wx_login_expired";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());
    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    // Create a user and bind openid.
    let email = unique_email("wx_expired");
    let user = webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
        .create_user(
            webshelf_server::repositories::user::CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Wx Expired".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user");

    let openid = "oWxExpired";
    wechat
        .login_service
        .bindings
        .bind_openid(&user.id.as_i64(), openid)
        .await
        .expect("Failed to bind openid");

    // Seed captcha with a 1-second TTL.
    let code = "EXPRD";
    let key = wechat.captcha_service.captcha_key(account_id, openid);
    wechat
        .captcha_store
        .set_captcha(&key, code, openid, std::time::Duration::from_secs(1))
        .await
        .expect("Failed to seed captcha");

    // Wait for the captcha to expire.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Try to use the expired captcha.
    let payload = serde_json::json!({ "code": code });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "expired captcha should return 400"
    );
    let body = body_to_json(response).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid or expired"),
        "expected captcha error, got: {:?}",
        body
    );
}

#[tokio::test]
#[ignore]
async fn test_captcha_len_configuration() {
    // Valid captcha code characters, matching wechat-api::captcha::CHARSET.
    let valid_chars: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

    let account_id = "test_captcha_len";
    let state = create_wechat_state_with_len(account_id, 6).await;
    let app = build_router(state);
    let token = "test_token";
    let ts = "1609459200";
    let nonce = "nonce_clen";
    let sig = compute_signature(token, ts, nonce);

    let uri = format!(
        "/api/public/wechat/callback?signature={}&timestamp={}&nonce={}",
        sig, ts, nonce
    );

    // XML body with a trigger keyword (验证码).
    let body = r#"<xml><ToUserName><![CDATA[gh_test]]></ToUserName><FromUserName><![CDATA[oCaptchaLenUser]]></FromUserName><CreateTime>1609459200</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[验证码]]></Content><MsgId>1234567892</MsgId></xml>"#;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header("content-type", "application/xml")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "callback with trigger keyword should return 200"
    );

    let text = body_text(response).await;
    let code = extract_code_from_reply(&text).expect("response must contain a verification code");

    assert_eq!(
        code.len(),
        6,
        "captcha code length must match configured captcha_len=6, got: '{}'",
        code
    );
    assert!(
        code.bytes().all(|b| valid_chars.contains(&b)),
        "code '{}' contains characters outside the valid charset",
        code
    );
}

#[tokio::test]
#[ignore]
async fn test_wx_login_concurrent_race() {
    // Two tasks try to consume the same captcha concurrently — only one
    // should succeed (the other gets a 400).
    let account_id = "test_wx_login_race";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());
    let app1 = app.clone();
    let app2 = app;

    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    // Create a user and bind openid.
    let email = unique_email("wx_race");
    let user = webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
        .create_user(
            webshelf_server::repositories::user::CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Wx Race".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user");

    let openid = "oWxRace";
    wechat
        .login_service
        .bindings
        .bind_openid(&user.id.as_i64(), openid)
        .await
        .expect("Failed to bind openid");

    // Seed a single captcha.
    let code = "RACE1";
    seed_captcha(&state, account_id, openid, code).await;

    let payload = serde_json::json!({ "code": code });
    let body = serde_json::to_string(&payload).unwrap();

    // Launch two concurrent login attempts.
    let body1 = body.clone();
    let h1 = tokio::spawn(async move {
        app1.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(body1))
                .unwrap(),
        )
        .await
        .unwrap()
    });
    let h2 = tokio::spawn(async move {
        app2.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
    });

    let (r1, r2) = tokio::join!(h1, h2);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();

    let statuses = [r1.status(), r2.status()];
    let ok_count = statuses.iter().filter(|&&s| s == StatusCode::OK).count();
    let bad_count = statuses
        .iter()
        .filter(|&&s| s == StatusCode::BAD_REQUEST)
        .count();

    assert_eq!(
        ok_count, 1,
        "exactly one concurrent request should succeed, got OK count = {}",
        ok_count
    );
    assert_eq!(
        bad_count, 1,
        "exactly one concurrent request should fail with 400, got BAD_REQUEST count = {}",
        bad_count
    );
}

#[tokio::test]
#[ignore]
async fn test_wx_login_user_deleted_after_captcha_seeded() {
    // Seed a captcha, then delete the user before calling wx-login.
    // The handler should return a generic error, not an internal leak.
    let account_id = "test_wx_login_deleted";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());
    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    // Create a user and bind openid.
    let email = unique_email("wx_deleted");
    let user = webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
        .create_user(
            webshelf_server::repositories::user::CreateUserInput {
                email: email.clone(),
                password: "Password123!".to_string(),
                name: "Wx Deleted".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user");

    let openid = "oWxDeleted";
    wechat
        .login_service
        .bindings
        .bind_openid(&user.id.as_i64(), openid)
        .await
        .expect("Failed to bind openid");

    // Seed a captcha.
    let code = "DELTD";
    seed_captcha(&state, account_id, openid, code).await;

    // Delete the user from the database via raw SQL.
    let user_id = user.id.as_i64();
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    state
        .db
        .write_conn()
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "DELETE FROM users WHERE id = $1",
            [user_id.into()],
        ))
        .await
        .expect("Failed to delete user");

    // Call wx-login — should fail with a generic error, no internal details.
    let payload = serde_json::json!({ "code": code });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // The captcha is valid and the openid is bound, but the user was
    // deleted — verify the error is generic (not an internal server error
    // leak). The correct behaviour: captcha is verified successfully,
    // then find_user_by_openid returns None, which maps to
    // ApiError::BadRequest("Invalid or expired captcha code").
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "deleted user should return 400, not 500"
    );

    let body = body_to_json(response).await;
    let msg = body["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("Invalid or expired"),
        "must return the same generic message as invalid captcha, got: {}",
        msg
    );
}

#[tokio::test]
#[ignore]
async fn test_login_with_captcha_mismatched_email() {
    // When WeChat captcha-login is enabled, a captcha obtained by user A
    // must NOT be usable to log in as user B via email+password.
    //
    // Flow:
    //   1. Create user A + user B, bind openid_A to user A
    //   2. Seed a captcha for user A
    //   3. Attempt login with user B's email+password + user A's captcha code
    //      → expect 400 (user IDs don't match)
    //   4. Verify user A CAN log in with A's own email+password + A's captcha
    //      → expect 200 (positive control)
    let account_id = "test_login_captcha_mismatch";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());
    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    // 1a. Create user A.
    let email_a = unique_email("mismatch_a");
    let user_a = webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
        .create_user(
            webshelf_server::repositories::user::CreateUserInput {
                email: email_a.clone(),
                password: "Password123!".to_string(),
                name: "Mismatch User A".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user A");

    // 1b. Create user B (different email and password).
    let email_b = unique_email("mismatch_b");
    let _user_b =
        webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
            .create_user(
                webshelf_server::repositories::user::CreateUserInput {
                    email: email_b.clone(),
                    password: "Password456!".to_string(),
                    name: "Mismatch User B".to_string(),
                    role: None,
                },
                "system",
            )
            .await
            .expect("Failed to create user B");

    // 2. Bind openid_A to user A only.
    let openid_a = "oMismatchUserA";
    wechat
        .login_service
        .bindings
        .bind_openid(&user_a.id.as_i64(), openid_a)
        .await
        .expect("Failed to bind openid A");

    // 3. Seed a captcha for user A.
    let code = "MISMT";
    seed_captcha(&state, account_id, openid_a, code).await;

    // ── Negative case: user B's email + user A's captcha → 400 ──────────
    let payload_mismatch = serde_json::json!({
        "email": email_b,
        "password": "Password456!",
        "captcha_code": code,
        "remember": false,
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&payload_mismatch).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "mismatched captcha user and email user must return 400"
    );
    let body = body_to_json(response).await;
    let msg = body["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("Invalid or expired"),
        "must return the same generic message as invalid captcha, got: {}",
        msg
    );

    // 4. ── Positive control: user A's email + user A's captcha → 200 ────
    // Seed another captcha for user A (the previous one was consumed by
    // verify_and_login in step 3 — but since the post-login check failed,
    // verify_and_login was NOT called for the mismatch path).
    // Actually, looking at the code flow in login_inner:
    //   1. captcha verification happens BEFORE password login
    //   2. verify_and_login consumes the captcha
    //   3. password login runs
    //   4. if user IDs mismatch, error is returned
    // So the captcha IS consumed even on failure. We need a fresh one.
    let code_ok = "MATCH";
    seed_captcha(&state, account_id, openid_a, code_ok).await;

    let payload_ok = serde_json::json!({
        "email": email_a,
        "password": "Password123!",
        "captcha_code": code_ok,
        "remember": false,
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload_ok).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "matching captcha user and email user must return 200"
    );

    let body = body_to_json(response).await;
    assert!(
        body["token"].as_str().map_or(false, |t| !t.is_empty()),
        "JWT must be issued on successful captcha-bound login"
    );
}

#[tokio::test]
#[ignore]
async fn test_login_valid_captcha_wrong_password() {
    // When WeChat captcha-login is enabled, a valid captcha code is
    // consumed by `verify_and_login` before the password check runs.
    // If the password is wrong, the captcha is consumed but the login
    // returns 401 (invalid credentials), not 400 (captcha error).
    //
    // Flow:
    //   1. Create user A, bind openid A
    //   2. Seed a valid captcha for user A
    //   3. Attempt login with user A's email + WRONG password + valid captcha
    //      → expect 401 (Unauthorized)
    //   4. Verify the captcha has been consumed (cannot be reused)
    let account_id = "test_login_captcha_wrong_pw";
    let state = create_wechat_state(account_id).await;
    let app = build_router(state.clone());
    let wechat = state.wechat.as_ref().expect("WeChat must be enabled");

    // 1. Create user A and bind openid.
    let email_a = unique_email("wx_wrong_pw");
    let user_a = webshelf_server::services::UserService::new(state.db.clone(), state.cache.clone())
        .create_user(
            webshelf_server::repositories::user::CreateUserInput {
                email: email_a.clone(),
                password: "Password123!".to_string(),
                name: "Wrong PW User".to_string(),
                role: None,
            },
            "system",
        )
        .await
        .expect("Failed to create user");

    let openid_a = "oWrongPW";
    wechat
        .login_service
        .bindings
        .bind_openid(&user_a.id.as_i64(), openid_a)
        .await
        .expect("Failed to bind openid");

    // 2. Seed a captcha for user A.
    let code = "WRNGP";
    seed_captcha(&state, account_id, openid_a, code).await;

    // 3. Attempt login with WRONG password + valid captcha → 401.
    let payload = serde_json::json!({
        "email": email_a,
        "password": "WrongPassword999!",
        "captcha_code": code,
        "remember": false,
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "wrong password must return 401, not 400 or 200"
    );
    let body = body_to_json(response).await;
    assert!(
        body["message"]
            .as_str()
            .map_or(false, |m| m.contains("Invalid email or password")),
        "must return generic auth error, got: {:?}",
        body
    );

    // 4. Verify captcha is consumed — try to reuse it via wx-login → 400.
    let payload_reuse = serde_json::json!({ "code": code });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/public/auth/wx-login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload_reuse).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "reusing a consumed captcha must return 400"
    );
}
