use axum::extract::{FromRequest, Request};
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

/// Maximum body size for form-data parsing (10 MB, matching RequestBodyLimitLayer).
const MAX_FORM_BODY_SIZE: usize = 10 * 1024 * 1024;

/// Custom extractor that accepts both `application/json` and
/// `application/x-www-form-urlencoded` request bodies.
///
/// Content-type detection is based on the `Content-Type` header:
/// - If the header contains `application/x-www-form-urlencoded`, the body is
///   parsed as URL-encoded form data.
/// - Otherwise (including missing header), the body is parsed as JSON.
pub struct JsonOrForm<T>(pub T);

#[derive(Debug)]
pub enum JsonOrFormRejection {
    Json(axum::extract::rejection::JsonRejection),
    Form(String),
}

impl IntoResponse for JsonOrFormRejection {
    fn into_response(self) -> Response {
        match self {
            Self::Json(rejection) => rejection.into_response(),
            Self::Form(msg) => (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Failed to parse form body: {msg}"),
            )
                .into_response(),
        }
    }
}

impl<S, T> FromRequest<S> for JsonOrForm<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = JsonOrFormRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        if content_type.contains("application/x-www-form-urlencoded") {
            let bytes = axum::body::to_bytes(req.into_body(), MAX_FORM_BODY_SIZE)
                .await
                .map_err(|e| JsonOrFormRejection::Form(e.to_string()))?;
            let value: T = serde_urlencoded::from_bytes(&bytes)
                .map_err(|e| JsonOrFormRejection::Form(e.to_string()))?;
            Ok(JsonOrForm(value))
        } else {
            // Default to JSON parsing (preserves existing behavior)
            let json = axum::Json::<T>::from_request(req, state)
                .await
                .map_err(JsonOrFormRejection::Json)?;
            Ok(JsonOrForm(json.0))
        }
    }
}
