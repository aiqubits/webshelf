//! Auth 状态层 —— JWT 解码、localStorage 持久化、AuthState 容器。

mod jwt;
mod state;
mod storage;

#[allow(unused_imports)]
pub use jwt::{JwtPayload, decode_payload, is_expired};
#[allow(unused_imports)]
pub use state::{AuthState, CurrentUser, now_unix_secs};
#[allow(unused_imports)]
pub use storage::{clear_token, load_token, save_token};
