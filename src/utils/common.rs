use serde::Serialize;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use crate::utils::ApiError;

/// Unified return type for business interfaces
#[derive(Serialize)]
pub struct AppResult<T : Serialize> {
    msg: String,
    status: u16,
    data: Option<T>
}


impl <T: Serialize> AppResult<T> {
    pub fn ok(data: T) -> AppResult<T> {
        AppResult {
            msg: String::from("success"),
            status: StatusCode::OK.as_u16(),
            data: Some(data)
        }
    }

    pub fn err(msg: String) -> AppResult<T> {
        Self::err2(msg,StatusCode::INTERNAL_SERVER_ERROR.as_u16())
    }

    pub fn err2(msg: String, status:u16) -> AppResult<T> {
        AppResult {
            msg: msg,
            status: status,
            data: None
        }
    }
}


impl <T: Serialize>IntoResponse for AppResult<T> {
    fn into_response(self) -> Response {
        serde_json::to_string(&self).expect("json parse error").into_response()
    }
}


/// Define the unified format for the data returned by the external interface
/// Add the new error type to the [ApiError] category.
/// Support for '?' syntax for rapid handling of exceptions
///
/// ## example
/// async fn test() -> R<T>
///
pub type R<T> = Result<AppResult<T>, ApiError>;

impl <T: Serialize> From<AppResult<T>> for R<T> {
    fn from(value: AppResult<T>) -> Self {
        Ok(value)
    }
}