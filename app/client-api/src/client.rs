use std::sync::{Arc, RwLock};

use reqwest::Method;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::error::{ClientError, ErrorBody};
use crate::types::*;

/// 指数退避延迟的最大位移量（shift 范围 0..=2），延迟序列：500ms, 1s, 2s
const MAX_BACKOFF_SHIFT: u32 = 2;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

/// 类型化 HTTP 客户端，封装所有 webshelf 后端 API 请求。
///
/// # Examples
///
/// ```rust,ignore
/// # use client_api::{Client, ClientConfig};
/// # // ⚠ 本示例需要真实后端，仅作 API 参考。
///
/// let client = Client::new(ClientConfig::new("http://localhost:8080"))?;
///
/// // 登录
/// let login = client.login("admin@example.com", "password123").await?;
/// client.set_token(login.token);
///
/// // 列出用户
/// let users = client.list_users(1, 20).await?;
/// for u in &users.items {
///     println!("{} <{}>", u.name, u.email);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

#[derive(Debug)]
struct ClientInner {
    client: reqwest::Client,
    config: ClientConfig,
    auth_token: RwLock<Option<String>>,
}

impl Client {
    /// 创建新的 API 客户端。
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        config.validate()?;

        let client = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(config.timeout_secs))
                    .build()
                    .map_err(|e| {
                        ClientError::Config(format!("Failed to create HTTP client: {}", e))
                    })?
            }
            #[cfg(target_arch = "wasm32")]
            {
                // WASM 下 reqwest 不支持设置超时（浏览器 fetch API 控制）
                let _ = config.timeout_secs;
                reqwest::Client::builder().build().map_err(|e| {
                    ClientError::Config(format!("Failed to create HTTP client: {}", e))
                })?
            }
        };

        Ok(Self {
            inner: Arc::new(ClientInner {
                client,
                config,
                auth_token: RwLock::new(None),
            }),
        })
    }

    // ──────────────────────────────────────────
    //  Token management（线程安全）
    // ──────────────────────────────────────────

    /// 设置认证 Token（线程安全，可在任意 `&self` 上下文中调用）。
    pub fn set_token(&self, token: impl Into<String>) {
        let mut guard = self.inner.auth_token.write().expect("RwLock poisoned");
        *guard = Some(token.into());
    }

    /// 清除认证 Token。
    pub fn clear_token(&self) {
        let mut guard = self.inner.auth_token.write().expect("RwLock poisoned");
        *guard = None;
    }

    /// 获取当前 Token 的副本。
    pub fn token(&self) -> Option<String> {
        let guard = self.inner.auth_token.read().expect("RwLock poisoned");
        guard.clone()
    }

    /// 检查是否已设置 Token。
    pub fn is_authenticated(&self) -> bool {
        let guard = self.inner.auth_token.read().expect("RwLock poisoned");
        guard.is_some()
    }

    /// 获取当前配置。
    pub fn config(&self) -> &ClientConfig {
        &self.inner.config
    }

    // ──────────────────────────────────────────
    //  Auth endpoints
    // ──────────────────────────────────────────

    /// 登录 — `POST /api/public/auth/login`
    pub async fn login(
        &self,
        email: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<LoginResponse, ClientError> {
        let body = LoginRequest {
            email: email.into(),
            password: password.into(),
        };
        self.post_json_no_auth("/api/public/auth/login", &body)
            .await
    }

    /// 注册 — `POST /api/public/auth/register`
    pub async fn register(
        &self,
        email: impl Into<String>,
        password: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<RegisterResponse, ClientError> {
        let body = RegisterRequest {
            email: email.into(),
            password: password.into(),
            name: name.into(),
        };
        self.post_json_no_auth("/api/public/auth/register", &body)
            .await
    }

    // ──────────────────────────────────────────
    //  Public endpoints
    // ──────────────────────────────────────────

    /// 健康检查 — `GET /api/health`
    pub async fn health_check(&self) -> Result<HealthResponse, ClientError> {
        self.get_json_no_auth("/api/health").await
    }

    // ──────────────────────────────────────────
    //  Admin: User management
    // ──────────────────────────────────────────

    /// 分页列出用户 — `GET /api/users?page=&per_page=`（需要 admin 角色）
    pub async fn list_users(
        &self,
        page: u64,
        per_page: u64,
    ) -> Result<PaginatedUsersResponse, ClientError> {
        if page == 0 || per_page == 0 {
            return Err(ClientError::Config(
                "page and per_page must be greater than 0".to_string(),
            ));
        }
        let url = self.inner.config.build_url("/api/users");
        let builder = self
            .request_with_auth(Method::GET, &url, None)?
            .query(&[("page", page), ("per_page", per_page)]);
        self.send_and_parse(builder).await
    }

    /// 获取单个用户 — `GET /api/users/{id}`（需要 admin 角色）
    pub async fn get_user(&self, id: Uuid) -> Result<UserResponse, ClientError> {
        self.get_json(&format!("/api/users/{}", id), None).await
    }

    /// 获取当前登录用户资料 — `GET /api/users/me`（任意已认证用户）
    pub async fn get_me(&self) -> Result<UserResponse, ClientError> {
        self.get_json("/api/users/me", None).await
    }

    /// 修改当前用户密码 — `POST /api/users/me/password`（任意已认证用户）
    pub async fn change_password(
        &self,
        current_password: impl Into<String>,
        new_password: impl Into<String>,
    ) -> Result<ChangePasswordResponse, ClientError> {
        let body = ChangePasswordRequest {
            current_password: current_password.into(),
            new_password: new_password.into(),
        };
        self.post_json("/api/users/me/password", &body, None).await
    }

    /// 创建用户 — `POST /api/users`（需要 admin 角色）
    pub async fn create_user(
        &self,
        email: impl Into<String>,
        password: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<UserResponse, ClientError> {
        let body = CreateUserRequest {
            email: email.into(),
            password: password.into(),
            name: name.into(),
        };
        self.post_json("/api/users", &body, None).await
    }

    /// 更新用户 — `PUT /api/users/{id}`（需要 admin 角色）
    ///
    /// 仅传入的字段会被更新；`None` 字段保持原值不变。
    pub async fn update_user(
        &self,
        id: Uuid,
        email: Option<String>,
        name: Option<String>,
        role: Option<String>,
    ) -> Result<UserResponse, ClientError> {
        let body = UpdateUserRequest { email, name, role };
        self.put_json(&format!("/api/users/{}", id), &body, None)
            .await
    }

    /// 删除用户 — `DELETE /api/users/{id}`（需要 admin 角色）
    pub async fn delete_user(&self, id: Uuid) -> Result<DeleteResponse, ClientError> {
        self.delete_json(&format!("/api/users/{}", id), None).await
    }

    // ──────────────────────────────────────────
    //  Internal HTTP helpers
    // ──────────────────────────────────────────

    /// 构建请求（不带认证头）。用于 login、register、health_check 等公共端点。
    fn request_no_auth(&self, method: Method, url: &str) -> reqwest::RequestBuilder {
        self.inner.client.request(method, url)
    }

    /// 构建带认证头的请求。
    ///
    /// 优先使用传入的 `token` 参数；如果未传入，使用内部存储的 token。
    fn request_with_auth(
        &self,
        method: Method,
        url: &str,
        token: Option<&str>,
    ) -> Result<reqwest::RequestBuilder, ClientError> {
        let mut builder = self.inner.client.request(method, url);

        if let Some(t) = token {
            builder = builder.header("Authorization", format!("Bearer {}", t));
        } else if let Some(t) = self.token() {
            builder = builder.header("Authorization", format!("Bearer {}", t));
        }

        Ok(builder)
    }

    /// GET 请求 + JSON 反序列化（不带认证）
    async fn get_json_no_auth<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let url = self.inner.config.build_url(path);
        let builder = self.request_no_auth(Method::GET, &url);
        self.send_and_parse(builder).await
    }

    /// GET 请求 + JSON 反序列化（带认证）
    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
    ) -> Result<T, ClientError> {
        let url = self.inner.config.build_url(path);
        let builder = self.request_with_auth(Method::GET, &url, token)?;
        self.send_and_parse(builder).await
    }

    /// POST 请求 + JSON 序列化/反序列化（不带认证）
    async fn post_json_no_auth<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let url = self.inner.config.build_url(path);
        let builder = self.request_no_auth(Method::POST, &url);
        self.send_and_parse(builder.json(body)).await
    }

    /// POST 请求 + JSON 序列化/反序列化（带认证）
    async fn post_json<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        token: Option<&str>,
    ) -> Result<T, ClientError> {
        let url = self.inner.config.build_url(path);
        let builder = self.request_with_auth(Method::POST, &url, token)?;
        self.send_and_parse(builder.json(body)).await
    }

    /// PUT 请求 + JSON 序列化/反序列化（带认证）
    async fn put_json<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        token: Option<&str>,
    ) -> Result<T, ClientError> {
        let url = self.inner.config.build_url(path);
        let builder = self.request_with_auth(Method::PUT, &url, token)?;
        self.send_and_parse(builder.json(body)).await
    }

    /// DELETE 请求 + JSON 反序列化（带认证）
    async fn delete_json<T: DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
    ) -> Result<T, ClientError> {
        let url = self.inner.config.build_url(path);
        let builder = self.request_with_auth(Method::DELETE, &url, token)?;
        self.send_and_parse(builder).await
    }

    /// 发送请求并解析 JSON 响应（含重试逻辑）。
    ///
    /// **重试条件**：网络/连接错误、服务器 5xx、限流 429
    ///
    /// **退避策略**：指数退避，初始延迟 500ms，每次翻倍，最大 2s
    ///
    /// 延迟序列（第 i 次重试）：500ms × 2^(i-1)，即 500ms, 1s, 2s（上限）
    async fn send_and_parse<T: DeserializeOwned>(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<T, ClientError> {
        let max_retries = self.inner.config.max_retries;

        // 带 streaming body 的 builder 无法克隆，直接发送不重试
        // 注意：不能加 max_retries > 0 的额外条件——
        // 如果 max_retries = 0 且 builder 不可克隆，会跳过整个发送，变成静默错误。
        if builder.try_clone().is_none() {
            let response = builder.send().await?;
            return Self::handle_response(response).await;
        }

        let mut last_err: Option<ClientError> = None;

        for attempt in 0..=max_retries {
            // 非首次尝试则加入指数退避延迟（包括最后一次重试，
            // 为服务器留出最大恢复窗口）
            if attempt > 0 {
                let delay_ms = 500u64 * (1u64 << (attempt - 1).min(MAX_BACKOFF_SHIFT));
                Self::sleep_ms(delay_ms.min(u32::MAX as u64) as u32).await;
            }

            // 入口守卫（第 329 行）已确保 builder 可克隆才进入循环；
            // try_clone 行为由 body 类型决定且不随迭代变化，此处必然成功。
            let req = builder
                .try_clone()
                .expect("builder known to be clonable (guarded at entry)");

            match req.send().await {
                Ok(response) => match Self::handle_response::<T>(response).await {
                    Ok(result) => return Ok(result),
                    Err(e) if Self::should_retry(&e) && attempt < max_retries => {
                        last_err = Some(e);
                        continue;
                    }
                    Err(e) => return Err(e),
                },
                Err(e) => {
                    let err: ClientError = e.into();
                    if Self::should_retry(&err) && attempt < max_retries {
                        last_err = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        // 理论上不可达：入口守卫确保仅可克隆 builder 才进入循环。
        // 保留此路径作为防御性编程，以防 reqwest 内部行为变更。
        Err(last_err.unwrap_or_else(|| {
            ClientError::Network("Request failed: all retries exhausted".to_string())
        }))
    }

    /// 判断错误是否值得重试。
    fn should_retry(err: &ClientError) -> bool {
        matches!(
            err,
            ClientError::Network(_) | ClientError::ServerError(..) | ClientError::RateLimited(_)
        )
    }

    /// 跨平台异步等待。
    ///
    /// - WASM：使用 `gloo_timers`
    /// - Native：使用 `tokio::time::sleep`
    async fn sleep_ms(ms: u32) {
        #[cfg(target_arch = "wasm32")]
        {
            gloo_timers::future::TimeoutFuture::new(ms).await;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        }
    }

    /// 统一处理 HTTP 响应：成功 → 反序列化 JSON，失败 → 构造对应错误。
    async fn handle_response<T: DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, ClientError> {
        let status = response.status();

        if status.is_success() {
            response.json::<T>().await.map_err(ClientError::from)
        } else {
            let status_code = status.as_u16();
            let text = response.text().await.unwrap_or_default();
            let default_msg = status.canonical_reason().unwrap_or("Unknown error");

            // 解析结构化错误体并构建消息，同时通过 from_status 正确分类
            let message = if let Ok(body) = serde_json::from_str::<ErrorBody>(&text) {
                format!("[{}] {}", body.error, body.message)
            } else if text.is_empty() {
                default_msg.to_string()
            } else {
                text
            };

            Err(ClientError::from_status(status_code, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_stored() {
        let config = ClientConfig::new("http://localhost:8080")
            .with_timeout(45)
            .with_max_retries(5);
        let client = Client::new(config.clone()).unwrap();

        assert_eq!(client.config().base_url, "http://localhost:8080");
        assert_eq!(client.config().timeout_secs, 45);
        assert_eq!(client.config().max_retries, 5);
    }

    #[test]
    fn test_token_management() {
        let client = Client::new(ClientConfig::new("http://localhost:8080")).unwrap();
        assert!(!client.is_authenticated());
        assert!(client.token().is_none());

        client.set_token("my-jwt-token");
        assert!(client.is_authenticated());
        assert_eq!(client.token(), Some("my-jwt-token".to_string()));

        client.clear_token();
        assert!(!client.is_authenticated());
    }

    #[test]
    fn test_token_shared_across_clones() {
        let client = Client::new(ClientConfig::new("http://localhost:8080")).unwrap();
        let cloned = client.clone();

        client.set_token("shared-token");
        assert_eq!(cloned.token(), Some("shared-token".to_string()));

        cloned.clear_token();
        assert!(client.token().is_none());
    }

    #[test]
    fn test_config_validation_http() {
        let config = ClientConfig::new("http://localhost:8080");
        assert!(Client::new(config).is_ok());
    }

    #[test]
    fn test_config_validation_https() {
        let config = ClientConfig::new("https://api.example.com");
        assert!(Client::new(config).is_ok());
    }

    // 原生平台上空 URL 不合法；WASM 下空 URL 通过 window.location 推导 origin，
    // 因此本测试仅在非 WASM 平台执行。
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_config_validation_empty_url() {
        let config = ClientConfig::new("");
        assert!(Client::new(config).is_err());
    }

    #[test]
    fn test_config_validation_rejects_ftp() {
        let config = ClientConfig::new("ftp://localhost");
        assert!(Client::new(config).is_err());
    }

    #[test]
    fn test_config_validation_rejects_zero_timeout() {
        let config = ClientConfig::new("http://localhost:8080").with_timeout(0);
        assert!(Client::new(config).is_err());
    }

    #[test]
    fn test_should_retry_network_error() {
        assert!(Client::should_retry(&ClientError::Network(
            "timeout".into()
        )));
    }

    #[test]
    fn test_should_retry_server_error() {
        assert!(Client::should_retry(&ClientError::ServerError(
            500,
            "Internal Server Error".into()
        )));
    }

    #[test]
    fn test_should_retry_rate_limited() {
        assert!(Client::should_retry(&ClientError::RateLimited(
            "Too many requests".into()
        )));
    }

    #[test]
    fn test_should_not_retry_client_error() {
        assert!(!Client::should_retry(&ClientError::Other(
            404,
            "Not found".into()
        )));
    }

    #[test]
    fn test_should_not_retry_config_error() {
        assert!(!Client::should_retry(&ClientError::Config("bad".into())));
    }
}
