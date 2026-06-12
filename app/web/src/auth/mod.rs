//! Auth 状态层 —— JWT 解码、localStorage 持久化、AuthState 容器。

/// JWT 过期判定的容差（与后端 `Validation.leeway` 保持一致）。
///
/// 后端 `server/src/middlewares/auth.rs` 接受 `exp + 5s` 之前的 token；
/// 前端同步加 5s 容差，避免在客户端时间略快或与服务器轻微时间差时
/// 前端提前 5s 把用户踢下线（Issue B2）。
pub const JWT_EXPIRY_LEEWAY_SECS: u64 = 5;

mod jwt;
mod state;
mod storage;

pub use jwt::{JwtPayload, decode_payload};
pub use state::{AuthState, CurrentUser};
pub use storage::{clear_token, load_token, save_token};
