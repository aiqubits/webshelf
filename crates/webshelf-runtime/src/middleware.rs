use crate::JwtClaims;

/// Application state accessor for adapter-level middleware.
///
/// Bridges `AppState` (defined in server crate) and adapter middleware
/// (defined in webshelf-axum) without circular dependency.
#[async_trait::async_trait]
pub trait MiddlewareState: Clone + Send + Sync + 'static {
    /// JWT secret key for token validation.
    fn jwt_secret(&self) -> &str;

    /// Whether to set Secure flag on cookies.
    fn cookie_secure(&self) -> bool;

    /// Verify that the token's `token_version` matches the user's current version.
    ///
    /// Uses Redis cache (30s TTL) with DB fallback.
    /// Must query the **write database** to guarantee read-your-writes consistency.
    async fn check_token_version(&self, user_id: i64, token_version: i32) -> Result<(), String>;
}

/// Validate JWT token using the state's secret.
pub fn validate_token(state: &impl MiddlewareState, token: &str) -> Result<JwtClaims, String> {
    crate::validate_jwt(token, state.jwt_secret())
}
