//! Auth 状态管理 —— 持有 `Client` 与当前用户信号，对外暴露 login / register / logout。

use client_api::{Client, ClientConfig, ClientError, LoginResponse, RegisterResponse};
use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::make_client;
use crate::auth::JWT_EXPIRY_LEEWAY_SECS;
use crate::auth::decode_payload;
use crate::components::now_unix_secs;

/// 注册流程的结果。
///
/// 服务端的 `POST /register` 会返回 `email_verified: bool`：
/// - `true`（SMTP 未配置 / 发送失败兜底）→ 用户已具备登录条件，前端自动 login 即可。
/// - `false`（SMTP 已配置）→ 用户必须先通过邮件验证才能登录，
///   前端需跳转验证页并暂存密码以便验证后自动登录。
///
/// `LoggedIn` 是单元变体而非携带 `LoginResponse` 的元组变体：
/// 实际的 `LoginResponse` 已被 `AuthState::login()` 内部消费（设置 token、
/// 拉取用户资料），视图层只需知道"已登录"这一个信号即可触发 `use_effect` 跳 `/`。
#[derive(Debug, Clone)]
pub enum RegisterOutcome {
    /// 服务端已自动验证，注册完毕即可登录。
    LoggedIn,
    /// 需要走邮件验证流程。`email` 用于构造 `/verify-email/{email}` 路由。
    NeedsVerification { email: String },
}

/// 注册期间的临时会话状态 —— 仅内存，不写 localStorage。
///
/// 用于在 `/auth` → `/verify-email/{email}` → 自动登录之间安全地
/// 暂存密码以完成"注册成功自动跳转到登录状态"的体验。
/// 密码在内存中存活约 1-2 分钟（用户输入验证码的时间），随后
/// verify-email 成功时立即被消费（调用 `login`），不会持久化到磁盘。
///
/// **安全警告**：明文密码驻留在 WASM 线性内存中，**同源 JS、DevTools、
/// heap dump 均可读取**。这是不可跳过的 UX 桥接代价——切勿将本结构
/// 复用于"记住我"等需要持久化的场景，也勿在任何 `console.log` /
/// `serde_json::to_string` 中暴露实例。
#[derive(Debug, Clone)]
pub struct PendingRegistration {
    pub email: String,
    pub password: String,
    pub remember: bool,
}

/// 判定一个 `ClientError` 是否为鉴权失败（401/403）。
///
/// 用于会话恢复：`/api/users/me` 收到 401/403 时清空会话而不是接受 JWT payload
/// 构造占位用户，避免“假登录” UI 状态（Issue #2）。
fn is_auth_failure(err: &ClientError) -> bool {
    matches!(err, ClientError::Other(401, _) | ClientError::Other(403, _))
}

/// 当前已登录用户。
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentUser {
    pub id: Uuid,
    pub role: String,
    pub name: String,
    pub email: String,
}

impl CurrentUser {
    /// 仅从 JWT 派生 id / role（name / email 占位，等待 /api/users/me 填充）。
    fn from_jwt(payload: &crate::auth::JwtPayload) -> Option<Self> {
        let id = Uuid::parse_str(&payload.sub).ok()?;
        Some(Self {
            id,
            role: payload.role.clone(),
            name: String::new(),
            email: String::new(),
        })
    }

    /// 用真实用户资料填充 name / email。
    fn with_profile(mut self, profile: &client_api::UserResponse) -> Self {
        self.name = profile.name.clone();
        self.email = profile.email.clone();
        self
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin" || self.role == "system"
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
    /// 初始化完成标志：restore_from_storage_async 结束后置 true。
    pub initialized: Signal<bool>,
    /// 注册期间的临时状态：仅内存，用于在 verify-email 成功后自动登录。
    /// 不写入 `localStorage` / `sessionStorage`。
    pub pending_registration: Signal<Option<PendingRegistration>>,
}

impl AuthState {
    /// 创建新的 `AuthState`（未登录状态）。
    ///
    /// 注意：不再在构造时同步恢复 localStorage；改为在 Auth 组件中通过
    /// `use_effect` 异步调用 `restore_from_storage_async()`，以便获取真实用户资料。
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
        }
    }

    /// 设置注册临时状态（仅内存）。
    pub fn set_pending_registration(&mut self, pending: PendingRegistration) {
        self.pending_registration.set(Some(pending));
    }

    /// 取出并清空注册临时状态。
    ///
    /// 在 verify-email 成功后调用，取出密码用于自动登录。
    /// `take` 语义保证密码被消费一次后从内存中消失。
    pub fn take_pending_registration(&mut self) -> Option<PendingRegistration> {
        self.pending_registration.write().take()
    }

    /// 仅查看当前 pending 状态（不清空）。
    #[allow(dead_code)]
    pub fn peek_pending_registration(&self) -> Option<PendingRegistration> {
        self.pending_registration.read().clone()
    }

    /// 主动清空注册临时状态。
    ///
    /// 调用场景：用户刷新 `/verify-email/{email}` 但密码已不在内存中；
    /// 用户从 `/auth` 进入新的注册流程以避免脏状态。
    pub fn clear_pending_registration(&mut self) {
        self.pending_registration.set(None);
    }

    /// 从 localStorage 恢复 token 并获取真实用户资料。
    ///
    /// 流程：读取 token → JWT 解码验证 → 设置 token → 调用 GET /api/users/me → 更新 user。
    /// 若任一步骤失败（token 缺失/过期/解码失败/API 错误），均视为未登录。
    pub async fn restore_from_storage_async(&mut self) {
        let Some(token) = crate::auth::load_token() else {
            self.initialized.set(true);
            return;
        };
        let Some((payload, _user_placeholder)) = parse_token(&token) else {
            crate::auth::clear_token();
            self.initialized.set(true);
            return;
        };
        if now_unix_secs() + JWT_EXPIRY_LEEWAY_SECS >= payload.exp {
            crate::auth::clear_token();
            self.initialized.set(true);
            return;
        }
        // Token 有效，先设置 token 使 client 可认证
        self.client.set_token(token.clone());
        self.token_expires_at.set(Some(payload.exp));

        // 调用 /api/users/me 获取真实用户资料
        match self.client.get_me().await {
            Ok(profile) => {
                let user = CurrentUser::from_jwt(&payload).map(|u| u.with_profile(&profile));
                self.user.set(user);
            }
            Err(err) => {
                // 鉴权失败（401/403）→ 清空会话，强制回到未登录态，
                // 避免"假登录"：用未通过服务端验证的 JWT payload 构造用户，
                // 让 UI 误以为已登录但所有后端调用都会被拒（Issue #2）。
                if is_auth_failure(&err) {
                    crate::auth::clear_token();
                    self.client.clear_token();
                    self.user.set(None);
                    self.token_expires_at.set(None);
                } else {
                    // 网络/解析/其他错误：保留 JWT 派生的占位用户，
                    // 至少 id / role 可用，UI 不至于完全空白。
                    let user = CurrentUser::from_jwt(&payload);
                    self.user.set(user);
                }
            }
        }
        self.initialized.set(true);
    }

    /// 登录。
    ///
    /// `remember = true` 时 token 写入 localStorage，下次启动自动恢复；
    /// `remember = false` 时 token 仅在内存中，刷新即丢。
    /// 登录成功后调用 GET /api/users/me 获取真实用户资料。
    pub async fn login(
        &mut self,
        email: &str,
        password: &str,
        remember: bool,
    ) -> Result<LoginResponse, ClientError> {
        let resp = self.client.login(email, password).await?;
        self.persist_session_async(&resp.token, remember).await;
        Ok(resp)
    }

    /// 注册。
    ///
    /// 根据服务端的 `email_verified` 字段分两条路径：
    /// - `true`（SMTP 未配置 / 发送失败兜底）：自动调用 `login` 写入会话。
    /// - `false`（SMTP 已配置）：返回 `NeedsVerification`，由视图层跳转验证页。
    ///
    /// 与旧实现的本质差异：旧实现在两种情况下都强制 login，导致 SMTP 已配置时
    /// 用户卡在"邮箱或密码错误"（`server/src/services/auth.rs:96-102` 拒绝未验证登录）。
    pub async fn register(
        &mut self,
        email: &str,
        password: &str,
        name: &str,
        remember: bool,
    ) -> Result<RegisterOutcome, ClientError> {
        let resp: RegisterResponse = self.client.register(email, password, name).await?;

        if resp.email_verified {
            // 服务端已自动验证：直接登录复用 login 的 token 持久化逻辑。
            // LoginResponse 已被 self.login() 内部消费，外部只关心"已登录"信号。
            self.login(email, password, remember).await?;
            Ok(RegisterOutcome::LoggedIn)
        } else {
            // 需要邮件验证：将密码暂存内存中，验证成功后再自动登录。
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

    /// 登出。清除 token、user、localStorage。
    pub fn logout(&mut self) {
        self.client.clear_token();
        self.user.set(None);
        self.token_expires_at.set(None);
        crate::auth::clear_token();
    }

    /// 持久化会话：设置 token + 调用 GET /api/users/me 获取真实用户资料。
    async fn persist_session_async(&mut self, token: &str, remember: bool) {
        if let Some((payload, _user_placeholder)) = parse_token(token) {
            self.client.set_token(token.to_string());
            self.token_expires_at.set(Some(payload.exp));

            // 尝试获取真实用户资料
            let user = match self.client.get_me().await {
                Ok(profile) => CurrentUser::from_jwt(&payload).map(|u| u.with_profile(&profile)),
                Err(err) => {
                    // 鉴权失败 → 拒绝持久化新会话，强制返回 Err 让上游走错误处理
                    // （Issue #2：与 restore 逻辑对齐，避免"假登录"）。
                    if is_auth_failure(&err) {
                        self.client.clear_token();
                        self.user.set(None);
                        self.token_expires_at.set(None);
                        crate::auth::clear_token();
                        return;
                    }
                    // 其他错误（网络/解析）：回退到 JWT 派生的占位用户
                    CurrentUser::from_jwt(&payload)
                }
            };
            self.user.set(user);

            if remember {
                crate::auth::save_token(token);
            } else {
                crate::auth::clear_token();
            }
        } else {
            // Token 格式异常（JWT 解码失败），放弃持久化以防不一致状态。
            crate::auth::clear_token();
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.user.read().is_some()
    }
}
