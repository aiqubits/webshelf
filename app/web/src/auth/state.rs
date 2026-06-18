//! Auth 状态管理 —— 持有 `Client` 与当前用户信号，对外暴露 login / register / logout。
//!
//! ## Token 存储策略
//!
//! JWT 和 refresh token 由后端通过 httpOnly cookie 下发（`webshelf_jwt`、
//! `webshelf_refresh`），浏览器自动管理，前端 JS 无法读取。
//!
//! 前端仅维护一个可读的 `webshelf_exp` cookie（存储 JWT 过期时间 Unix 秒），
//! 用于 UI 层的过期检测和自动刷新决策。
//!
//! Client 的 `auth_token` 仍保留用于 Authorization 头（非浏览器场景兼容），
//! 但浏览器请求主要通过 httpOnly cookie 认证。

use client_api::{Client, ClientConfig, ClientError, LoginResponse, RegisterResponse};
use dioxus::prelude::*;

use crate::api::make_client;
use crate::auth::JWT_EXPIRY_LEEWAY_SECS;
use crate::auth::decode_payload;
use crate::components::now_unix_secs;

/// 注册流程的结果。
#[derive(Debug, Clone)]
pub enum RegisterOutcome {
    /// 服务端已自动验证，注册完毕即可登录。
    LoggedIn,
    /// 需要走邮件验证流程。`email` 用于构造 `/verify-email/{email}` 路由。
    NeedsVerification { email: String },
}

/// 注册期间的临时会话状态 —— 仅内存，不写 cookie。
#[derive(Debug, Clone)]
pub struct PendingRegistration {
    pub email: String,
    pub password: String,
    pub remember: bool,
}

/// 判定一个 `ClientError` 是否为鉴权失败（401/403）。
fn is_auth_failure(err: &ClientError) -> bool {
    matches!(err, ClientError::Other(401, _) | ClientError::Other(403, _))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_failure_401() {
        assert!(is_auth_failure(&ClientError::Other(401, "x".into())));
    }

    #[test]
    fn auth_failure_403() {
        assert!(is_auth_failure(&ClientError::Other(403, "x".into())));
    }

    #[test]
    fn auth_failure_400_is_not() {
        assert!(!is_auth_failure(&ClientError::Other(400, "x".into())));
    }

    #[test]
    fn auth_failure_500_is_not() {
        assert!(!is_auth_failure(&ClientError::ServerError(500, "x".into())));
    }

    #[test]
    fn auth_failure_network_is_not() {
        assert!(!is_auth_failure(&ClientError::Network("timeout".into())));
    }
}

/// 当前已登录用户。
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentUser {
    pub id: String,
    pub role: String,
    pub name: String,
    pub email: String,
    pub balance: i64,
}

impl CurrentUser {
    fn from_jwt(payload: &crate::auth::JwtPayload) -> Option<Self> {
        if payload.sub.is_empty() {
            return None;
        }
        Some(Self {
            id: payload.sub.clone(),
            role: payload.role.clone(),
            name: String::new(),
            email: String::new(),
            balance: 0,
        })
    }

    fn with_profile(mut self, profile: &client_api::UserResponse) -> Self {
        self.name = profile.name.clone();
        self.email = profile.email.clone();
        self.balance = profile.balance;
        self
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin" || self.role == "system"
    }

    pub fn is_system(&self) -> bool {
        self.role == "system"
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
    pub client: Client,
    pub user: Signal<Option<CurrentUser>>,
    /// JWT 过期时间（Unix 秒）。`None` 表示未登录。
    pub token_expires_at: Signal<Option<u64>>,
    pub initialized: Signal<bool>,
    pub pending_registration: Signal<Option<PendingRegistration>>,
    /// 静默刷新进行中标志——防止并发 refresh 请求。
    refreshing: Signal<bool>,
}

impl AuthState {
    pub fn new() -> Self {
        let client = make_client().unwrap_or_else(|_| {
            Client::new(ClientConfig::default())
                .expect("default client config should always be valid")
        });
        Self {
            client,
            user: Signal::new(None),
            token_expires_at: Signal::new(None),
            initialized: Signal::new(false),
            pending_registration: Signal::new(None),
            refreshing: Signal::new(false),
        }
    }

    pub fn set_pending_registration(&mut self, pending: PendingRegistration) {
        self.pending_registration.set(Some(pending));
    }

    pub fn take_pending_registration(&mut self) -> Option<PendingRegistration> {
        self.pending_registration.write().take()
    }

    #[allow(dead_code)]
    pub fn peek_pending_registration(&self) -> Option<PendingRegistration> {
        self.pending_registration.read().clone()
    }

    pub fn clear_pending_registration(&mut self) {
        self.pending_registration.set(None);
    }

    /// 从 cookie 恢复会话并获取真实用户资料。
    ///
    /// 流程：读取 `webshelf_exp` cookie 获取过期时间 → 检查是否过期
    /// → 调用 GET /api/users/me（浏览器自动发送 httpOnly JWT cookie）
    /// → 更新 user。
    ///
    /// 如果 JWT 已过期但 refresh token 有效，尝试静默刷新。
    pub async fn restore_from_storage_async(&mut self) {
        let Some(expires_at) = crate::auth::load_token() else {
            self.initialized.set(true);
            return;
        };

        let now = now_unix_secs();

        if now + JWT_EXPIRY_LEEWAY_SECS >= expires_at {
            // Token 已过期，尝试静默刷新
            if self.try_refresh_async().await {
                // 刷新成功，try_refresh_async 已更新 token_expires_at 为新值，
                // 不要再用旧的 expires_at 覆盖它
            } else {
                // 刷新失败，清除会话
                crate::auth::clear_token();
                self.initialized.set(true);
                return;
            }
        } else {
            // Token 尚未过期，直接用 cookie 中的过期时间
            self.token_expires_at.set(Some(expires_at));
        }

        // 调用 /api/users/me 获取真实用户资料
        // 浏览器会自动发送 httpOnly JWT cookie，无需手动设置 Authorization 头
        match self.client.get_me().await {
            Ok(profile) => {
                // 从 cookie 中读取 JWT 来解码 payload 获取 user_id/role
                // 由于 httpOnly cookie 无法从 JS 读取，我们用 get_me 的响应构造用户
                let user = Some(CurrentUser {
                    id: profile.id.to_string(),
                    role: profile.role.clone(),
                    name: profile.name.clone(),
                    email: profile.email.clone(),
                    balance: profile.balance,
                });
                self.user.set(user);
            }
            Err(err) => {
                if is_auth_failure(&err) {
                    // 尝试刷新
                    if self.try_refresh_async().await {
                        // 刷新成功后重试
                        match self.client.get_me().await {
                            Ok(profile) => {
                                let user = Some(CurrentUser {
                                    id: profile.id.to_string(),
                                    role: profile.role.clone(),
                                    name: profile.name.clone(),
                                    email: profile.email.clone(),
                                    balance: profile.balance,
                                });
                                self.user.set(user);
                            }
                            Err(retry_err) => {
                                if is_auth_failure(&retry_err) {
                                    // 重试仍然鉴权失败 → 会话真正死亡
                                    crate::auth::clear_token();
                                    self.client.clear_token();
                                    self.user.set(None);
                                    self.token_expires_at.set(None);
                                } else {
                                    // 网络层错误：JWT 已刷新成功，用本地 token 兜底构造用户
                                    if let Some(token) = self.client.token()
                                        && let Some((_payload, user)) = parse_token(&token)
                                    {
                                        self.user.set(Some(user));
                                    }
                                }
                            }
                        }
                    } else {
                        crate::auth::clear_token();
                        self.client.clear_token();
                        self.user.set(None);
                        self.token_expires_at.set(None);
                    }
                } else {
                    // 网络错误：保留过期时间，UI 至少知道曾经登录过
                    self.token_expires_at.set(Some(expires_at));
                }
            }
        }
        self.initialized.set(true);
    }

    /// 尝试静默刷新 JWT。
    ///
    /// 调用 `POST /api/public/auth/refresh`，依赖浏览器自动发送
    /// `webshelf_refresh` httpOnly cookie。
    ///
    /// 成功时更新 `token_expires_at` 和 client token。
    /// 失败时返回 false，调用方应清除会话。
    ///
    /// 并发安全：当检测到已有刷新在进行中时，等待其完成并检查结果，
    /// 而非立即返回 true（避免调用方在 JWT 尚未更新时发出 API 请求）。
    pub async fn try_refresh_async(&mut self) -> bool {
        if *self.refreshing.read() {
            // 另一个刷新正在进行中 —— 等待其完成，避免在 JWT 尚未更新的
            // 窗口期内向调用方返回 true（会导致调用方发出 401 请求）。
            // WASM 单线程环境下，sleep 让出执行权给进行中的刷新任务。
            for _ in 0..60 {
                client_api::Client::sleep_ms(200).await;
                if !*self.refreshing.read() {
                    return self
                        .token_expires_at
                        .cloned()
                        .is_some_and(|exp| now_unix_secs() + JWT_EXPIRY_LEEWAY_SECS < exp);
                }
            }
            // 刷新卡住（超过 12 秒）—— 返回 false 触发登出
            return false;
        }
        self.refreshing.set(true);
        let result = match self.client.refresh().await {
            Ok(resp) => {
                let expires_at = now_unix_secs() + resp.expires_in;
                self.client.set_token(&resp.token);
                self.token_expires_at.set(Some(expires_at));
                crate::auth::save_token(expires_at, resp.refresh_expires_in.max(resp.expires_in));
                true
            }
            Err(err) => {
                #[cfg(target_arch = "wasm32")]
                web_sys::console::warn_1(&format!("Silent token refresh failed: {:?}", err).into());
                let _ = err;
                false
            }
        };
        self.refreshing.set(false);
        result
    }

    /// 登录。
    ///
    /// `remember = true` 时后端签发 30 天 JWT + 90 天 refresh token，
    /// 均通过 httpOnly cookie 下发；`remember = false` 时签发 1 小时 JWT。
    /// 前端保存 JWT 过期时间到可读 cookie。
    pub async fn login(
        &mut self,
        email: &str,
        password: &str,
        remember: bool,
    ) -> Result<LoginResponse, ClientError> {
        let resp = self.client.login(email, password, remember).await?;
        let expires_at = now_unix_secs() + resp.expires_in;
        self.client.set_token(&resp.token);
        self.token_expires_at.set(Some(expires_at));

        // 保存过期时间到可读 cookie（JWT 本身在 httpOnly cookie 中）
        // 使用 refresh_expires_in 作为 Max-Age，确保 webshelf_exp 在 refresh token
        // 有效期内一直存在，前端在 JWT 过期后仍能触发静默刷新。
        crate::auth::save_token(
            expires_at,
            resp.refresh_expires_in.unwrap_or(0).max(resp.expires_in),
        );

        // 获取真实用户资料
        match self.client.get_me().await {
            Ok(profile) => {
                let user = Some(CurrentUser {
                    id: profile.id.to_string(),
                    role: profile.role.clone(),
                    name: profile.name.clone(),
                    email: profile.email.clone(),
                    balance: profile.balance,
                });
                self.user.set(user);
            }
            Err(err) => {
                if is_auth_failure(&err) {
                    self.client.clear_token();
                    self.user.set(None);
                    self.token_expires_at.set(None);
                    crate::auth::clear_token();
                    return Err(err);
                }
            }
        }
        Ok(resp)
    }

    /// 注册。
    pub async fn register(
        &mut self,
        email: &str,
        password: &str,
        name: &str,
        remember: bool,
    ) -> Result<RegisterOutcome, ClientError> {
        let resp: RegisterResponse = self
            .client
            .register(email, password, name, remember)
            .await?;

        if resp.email_verified {
            self.login(email, password, remember).await?;
            Ok(RegisterOutcome::LoggedIn)
        } else {
            self.set_pending_registration(PendingRegistration {
                email: email.to_string(),
                password: password.to_string(),
                remember,
            });
            Ok(RegisterOutcome::NeedsVerification {
                email: email.to_string(),
            })
        }
    }

    /// 仅轮转 JWT，不重新拉取用户资料。
    ///
    /// 用于 `change_password` / `reset_password` 场景。
    pub fn swap_token(&mut self, new_token: impl Into<String>) {
        let new_token = new_token.into();
        let Some((payload, _user_placeholder)) = parse_token(&new_token) else {
            self.logout();
            return;
        };
        if now_unix_secs() + JWT_EXPIRY_LEEWAY_SECS >= payload.exp {
            self.logout();
            return;
        }
        self.client.set_token(new_token);
        self.token_expires_at.set(Some(payload.exp));
        // 沿用旧会话的持久化偏好
        if crate::auth::load_token().is_some() {
            crate::auth::save_token(payload.exp, payload.exp.saturating_sub(now_unix_secs()));
        } else {
            crate::auth::clear_token();
        }
    }

    /// 登出。清除 token、user、cookie。
    ///
    /// 同步、纯本地状态清理 —— 不发后端请求。适用于被同步代码路径调用
    /// 的"防御性登出"场景（如 `swap_token` 检测到新 token 异常）。
    /// 真正要撤销 refresh token 的"用户主动登出"或"会话过期"场景应使用
    /// `logout_async`，它会调用后端 `/logout` 删除 refresh token 行。
    pub fn logout(&mut self) {
        self.client.clear_token();
        self.user.set(None);
        self.token_expires_at.set(None);
        crate::auth::clear_token();
    }

    /// 登出（异步）—— 调用后端 `POST /api/public/auth/logout` 撤销
    /// refresh token，再清除本地状态。
    ///
    /// 后端错误一律吞掉 —— 因为即使请求失败，本地状态
    /// 仍然需要清空，否则会出现"本地已登出但后端 refresh 仍然有效"
    /// 的悬空会话。
    pub async fn logout_async(&mut self) {
        if let Err(ref e) = self.client.logout().await {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(
                &format!(
                    "Logout API call failed (local state cleared anyway): {:?}",
                    e
                )
                .into(),
            );
            let _ = e;
        }
        self.logout();
    }

    /// 登出所有设备（异步）—— 调用后端 `POST /api/users/me/logout-all`
    /// 撤销所有 refresh token + 递增 token_version，再清除本地状态。
    ///
    /// 与 `logout_async` 的差异：
    /// - 后端会递增 `token_version` 使所有设备的 JWT 立即失效
    /// - 删除所有 refresh token（包括当前设备）
    /// - 不应在会话过期或被动登出时调用，仅用于用户主动点击"登出所有设备"
    pub async fn logout_all_async(&mut self) {
        if let Err(ref e) = self.client.logout_all().await {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(
                &format!(
                    "Logout-all API call failed (local state cleared anyway): {:?}",
                    e
                )
                .into(),
            );
            let _ = e;
        }
        self.logout();
    }

    /// 持久化会话：设置 token + 调用 GET /api/users/me 获取真实用户资料。
    ///
    /// 由 `reset_password` 视图复用。
    pub async fn persist_session_async(&mut self, token: &str, remember: bool) {
        if let Some((payload, _user_placeholder)) = parse_token(token) {
            self.client.set_token(token.to_string());
            self.token_expires_at.set(Some(payload.exp));

            let user = match self.client.get_me().await {
                Ok(profile) => CurrentUser::from_jwt(&payload).map(|u| u.with_profile(&profile)),
                Err(err) => {
                    if is_auth_failure(&err) {
                        self.client.clear_token();
                        self.user.set(None);
                        self.token_expires_at.set(None);
                        crate::auth::clear_token();
                        return;
                    }
                    CurrentUser::from_jwt(&payload)
                }
            };
            self.user.set(user);

            if remember {
                let now = now_unix_secs();
                crate::auth::save_token(payload.exp, payload.exp.saturating_sub(now));
            } else {
                crate::auth::clear_token();
            }
        } else {
            crate::auth::clear_token();
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.user.read().is_some()
    }
}
