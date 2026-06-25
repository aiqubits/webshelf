//! Handler helper functions — reduce boilerplate in handler implementations.
//!
//! # Why this module exists
//!
//! Every authenticated handler starts with the same two lines:
//! ```ignore
//! let state: AppState = req.get_data().ok_or_else(|| HttpError::internal("..."))?;
//! let auth_user: AuthUser = req.get_data().ok_or_else(|| HttpError::unauthorized("..."))?;
//! ```
//!
//! These helper functions centralise this pattern, reducing duplication
//! and making AI-generated handlers more concise.

use crate::AppState;
use crate::middlewares::AuthUser;
use webshelf_runtime::{HttpError, RequestContext};

/// Extract `AppState` from a request context.
///
/// Intended for **public** (unauthenticated) handlers where only the
/// application state is needed.
pub fn extract_state(req: &crate::ServerRequest) -> Result<AppState, HttpError> {
    req.get_data()
        .ok_or_else(|| HttpError::internal("AppState not available"))
}

/// Extract `(AppState, AuthUser)` from a request context.
///
/// Intended for **authenticated** handlers. Returns:
/// - `HttpError::internal` if AppState is missing (should never happen at runtime)
/// - `HttpError::unauthorized` if AuthUser is missing (request did not pass auth middleware)
pub fn extract_handler_context(
    req: &crate::ServerRequest,
) -> Result<(AppState, AuthUser), HttpError> {
    let state: AppState = req
        .get_data()
        .ok_or_else(|| HttpError::internal("AppState not available"))?;
    let auth_user: AuthUser = req
        .get_data()
        .ok_or_else(|| HttpError::unauthorized("Authentication required"))?;
    Ok((state, auth_user))
}
