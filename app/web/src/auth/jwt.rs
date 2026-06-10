//! JWT payload 解码 —— 仅用于 UI 派生当前用户信息。
//!
//! **不做签名校验**：后端是唯一鉴权权威；前端只读取 `sub` / `role` / `exp` 用于 UI 状态。

use base64::Engine;
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct JwtPayload {
    /// 用户 UUID（字符串）
    pub sub: String,
    /// Unix 秒过期时间
    pub exp: u64,
    /// Unix 秒签发时间
    #[serde(default)]
    pub iat: u64,
    /// "user" | "admin"
    pub role: String,
}

/// 解码 JWT payload。失败时返回 `None`。
///
/// 解析过程中任何错误（格式错误、base64 错误、JSON 错误、字段缺失）
/// 都会被视为无效 token，调用方应清除并跳转登录。
pub fn decode_payload(token: &str) -> Option<JwtPayload> {
    let payload_b64 = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    serde_json::from_slice::<JwtPayload>(&bytes).ok()
}

/// 检查 token 是否已过期（基于 JWT 的 `exp` 字段）。
pub fn is_expired(token: &str, now_unix_secs: u64) -> bool {
    decode_payload(token).is_none_or(|p| now_unix_secs >= p.exp)
}
