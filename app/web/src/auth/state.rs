//! Auth 状态管理 —— 持有 `Client` 与当前用户信号，对外暴露 login / register / logout。

use client_api::{Client, ClientError, LoginResponse, RegisterResponse};
use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::make_client;
use crate::auth::{decode_payload, is_expired};

/// 当前已登录用户。
///
/// 注意：服务器没有 `/me` 端点；`name` / `email` 字段由前端根据 role 派生，
/// 将在 Phase 3 接入 `GET /api/users/{id}` 后替换为真实值。
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentUser {
    pub id: Uuid,
    pub role: String,
    pub name: String,
    pub email: String,
}

impl CurrentUser {
    fn from_jwt(payload: &crate::auth::JwtPayload) -> Option<Self> {
        let id = Uuid::parse_str(&payload.sub).ok()?;
        let is_admin = payload.role == "admin";
        Some(Self {
            id,
            role: payload.role.clone(),
            name: if is_admin {
                "WebShelf Admin".into()
            } else {
                "WebShelf User".into()
            },
            email: if is_admin {
                "admin@webshelf.dev".into()
            } else {
                "user@webshelf.dev".into()
            },
        })
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// 一次解码 token，返回 `(payload, user)`。失败时返回 `None`。
fn parse_token(token: &str) -> Option<(crate::auth::JwtPayload, CurrentUser)> {
    let payload = decode_payload(token)?;
    let user = CurrentUser::from_jwt(&payload)?;
    Some((payload, user))
}

/// Auth 全局状态。应在 `App` 组件中创建一次并通过 `use_context_provider` 注入。
#[derive(Clone)]
pub struct AuthState {
    /// 共享的 API 客户端（已登录时携带 token）。
    pub client: Client,
    /// 当前用户；未登录时为 `None`。
    pub user: Signal<Option<CurrentUser>>,
    /// token 过期时间（Unix 秒）。`None` 表示未登录。
    pub token_expires_at: Signal<Option<u64>>,
}

impl AuthState {
    /// 创建新的 `AuthState`（未登录状态），并尝试从 localStorage 恢复。
    pub fn new() -> Self {
        let client = make_client().expect("client-api config should be valid");
        let user = Signal::new(None);
        let token_expires_at = Signal::new(None);
        let mut state = Self {
            client,
            user,
            token_expires_at,
        };
        state.restore_from_storage();
        state
    }

    /// 从 localStorage 恢复 token。token 缺失 / 过期 / 解码失败均视为未登录。
    pub fn restore_from_storage(&mut self) {
        let Some(token) = crate::auth::load_token() else {
            return;
        };
        if is_expired(&token, now_unix_secs()) {
            crate::auth::clear_token();
            return;
        }
        let Some((payload, user)) = parse_token(&token) else {
            crate::auth::clear_token();
            return;
        };
        self.client.set_token(token);
        self.user.set(Some(user));
        self.token_expires_at.set(Some(payload.exp));
    }

    /// 登录。
    ///
    /// `remember = true` 时 token 写入 localStorage，下次启动自动恢复；
    /// `remember = false` 时 token 仅在内存中，刷新即丢。
    pub async fn login(
        &mut self,
        email: &str,
        password: &str,
        remember: bool,
    ) -> Result<LoginResponse, ClientError> {
        let resp = self.client.login(email, password).await?;
        self.persist_session(&resp.token, remember);
        Ok(resp)
    }

    /// 注册。注册成功后立刻调用 login（服务器不会自动签发 token）。
    pub async fn register(
        &mut self,
        email: &str,
        password: &str,
        name: &str,
        remember: bool,
    ) -> Result<LoginResponse, ClientError> {
        let _register_resp: RegisterResponse = self.client.register(email, password, name).await?;
        // 注册成功后自动登录，复用 login 的 token 持久化逻辑。
        self.login(email, password, remember).await
    }

    /// 登出。清除 token、user、localStorage。
    pub fn logout(&mut self) {
        self.client.clear_token();
        self.user.set(None);
        self.token_expires_at.set(None);
        crate::auth::clear_token();
    }

    fn persist_session(&mut self, token: &str, remember: bool) {
        self.client.set_token(token);
        if let Some((payload, user)) = parse_token(token) {
            self.user.set(Some(user));
            self.token_expires_at.set(Some(payload.exp));
        }
        if remember {
            crate::auth::save_token(token);
        } else {
            crate::auth::clear_token();
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.user.read().is_some()
    }
}

/// 当前 Unix 秒（用于判断 JWT 是否过期）。
pub fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
