//! API 客户端，封装 webshelf 后端所有 REST 接口。
//!
//! # 快速开始
//!
//! ```rust,no_run
//! # use client_api::{Client, ClientConfig};
//! # async fn _doctest() -> Result<(), Box<dyn std::error::Error>> {
//! # // ⚠ 本示例需要真实后端，仅作 API 参考。
//! // 原生平台：指定后端地址（本地开发用 localhost，生产用域名）
//! let client = Client::new(ClientConfig::new("http://127.0.0.1:8080"))?;
//!
//! // 注意：空 base_url（相对路径）仅在 WASM（浏览器）环境下有效，
//! // 原生平台（桌面/移动端）必须使用完整的 http:// 或 https:// URL。
//!
//! // 登录
//! let login = client.login("admin@example.com", "password123", false, None::<String>).await?;
//! client.set_token(login.token);
//!
//! // 列出用户（admin 权限）
//! let users = client.list_users(1, 20).await?;
//! for u in &users.items {
//!     println!("{} <{}>", u.name, u.email);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Builder 模式
//!
//! ```rust,no_run
//! use client_api::{Client, ClientConfig};
//!
//! let config = ClientConfig::new("http://127.0.0.1:8080")
//!     .with_timeout(60)
//!     .with_max_retries(5);
//! let client = Client::new(config).expect("valid config");
//! ```

mod client;
pub mod config;
pub mod error;
pub mod types;

pub use client::Client;
pub use config::ClientConfig;
pub use error::ClientError;
pub use types::*;
