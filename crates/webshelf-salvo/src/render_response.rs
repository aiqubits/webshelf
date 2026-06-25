use salvo::Response as SalvoResponse;
use salvo::http::HeaderName;
use salvo::http::StatusCode;
use webshelf_runtime::{Response, ResponseBody};

pub fn render_response(mut resp: Response, res: &mut SalvoResponse) {
    res.status_code(resp.status());

    // 1. Determine Content-Type: explicit override > auto-detect
    let content_type = resp.content_type().unwrap_or_else(|| {
        if matches!(resp.body(), ResponseBody::Json(_)) {
            "application/json; charset=utf-8"
        } else {
            "text/plain; charset=utf-8"
        }
    });

    // Use from_static — "content-type" is a well-known, lowercase HTTP header name.
    let ct_name = HeaderName::from_static("content-type");
    let ct_value = salvo::http::HeaderValue::from_str(content_type).unwrap_or(
        salvo::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    res.headers_mut().insert(ct_name, ct_value);

    // 2. Set all other headers (cookie 已存储在 headers 中)。
    //    过滤 Content-Type 以免与上面的显式设置冲突。
    //    使用 append 而非 insert：Set-Cookie 可能有多个值
    //    （JWT cookie + Refresh cookie + Expiry cookie）。
    for (name, value) in resp.take_headers() {
        if name.as_str().eq_ignore_ascii_case("content-type") {
            continue;
        }
        if let Ok(parsed) = value.parse::<salvo::http::HeaderValue>() {
            res.headers_mut().append(name, parsed);
        }
    }

    // 3. Write body — use write_body for raw bytes (no UTF-8 corruption).
    //    write_body handles all ResBody variants (None → Once(bytes)).
    match resp.read_bytes() {
        Ok(bytes) => {
            if !bytes.is_empty()
                && let Err(e) = res.write_body(bytes)
            {
                tracing::error!("Failed to write response body: {}", e);
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
        Err(e) => {
            tracing::error!("Failed to serialize response body: {}", e);
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn render_json_response_detects_content_type() {
        let mut unified = Response::new();
        unified.set_json_body(serde_json::json!({"key": "value"}));

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        assert_eq!(salvo_res.status_code, Some(StatusCode::OK));
        let ct = salvo_res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert_eq!(ct, Some("application/json; charset=utf-8"));
    }

    #[test]
    fn render_text_response_detects_content_type() {
        let mut unified = Response::new();
        unified.set_text_body("hello");

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        assert_eq!(salvo_res.status_code, Some(StatusCode::OK));
        let ct = salvo_res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert_eq!(ct, Some("text/plain; charset=utf-8"));
    }

    #[test]
    fn render_response_explicit_content_type_override() {
        let mut unified = Response::new();
        unified.set_json_body(serde_json::json!({"key": "value"}));
        unified.set_content_type("application/pdf");

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        let ct = salvo_res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert_eq!(ct, Some("application/pdf"));
    }

    #[test]
    fn render_response_sets_status_code() {
        use http::StatusCode as HttpStatusCode;
        let mut unified = Response::with_status(HttpStatusCode::NOT_FOUND);
        unified.set_json_body(serde_json::json!({"error": "not_found"}));

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        assert_eq!(salvo_res.status_code, Some(StatusCode::NOT_FOUND));
    }

    #[test]
    fn render_response_appends_multiple_set_cookie_headers() {
        use cookie::Cookie;
        let mut unified = Response::new();
        unified.set_text_body("ok");

        unified.set_cookie(Cookie::new("session", "abc123"));
        unified.set_cookie(Cookie::new("refresh", "xyz789"));

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        let cookies: Vec<&str> = salvo_res
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();

        assert_eq!(cookies.len(), 2, "Should have two set-cookie headers");
        assert!(
            cookies.iter().any(|c| c.starts_with("session=")),
            "Should contain session cookie"
        );
        assert!(
            cookies.iter().any(|c| c.starts_with("refresh=")),
            "Should contain refresh cookie"
        );
    }

    #[test]
    fn render_empty_response_sets_no_body() {
        let unified = Response::new(); // Empty body, OK status

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        assert_eq!(salvo_res.status_code, Some(StatusCode::OK));
        // Empty body should remain empty (no write_body call)
        assert!(matches!(salvo_res.body, salvo::http::body::ResBody::None));
    }

    #[test]
    fn render_http_error_response() {
        use webshelf_runtime::HttpError;
        let err = HttpError::bad_request("invalid input");
        let unified: Response = err.into();

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        assert_eq!(salvo_res.status_code, Some(StatusCode::BAD_REQUEST));

        // Verify body contains error info
        match &salvo_res.body {
            salvo::http::body::ResBody::Once(bytes) => {
                let body_str = std::str::from_utf8(bytes).unwrap();
                assert!(body_str.contains("bad_request"));
                assert!(body_str.contains("invalid input"));
            }
            _ => panic!("Expected ResBody::Once"),
        }
    }

    #[test]
    fn render_response_content_type_filtered_from_headers() {
        // Verify that Content-Type from take_headers() is not duplicated
        // (it's set explicitly before the loop).
        let mut unified = Response::new();
        unified.set_json_body(serde_json::json!({}));
        // Set content-type via generic header path (should be filtered)
        unified.insert_header("content-type", "text/html");

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        // Should keep the auto-detected JSON content-type, not the manually set one
        let ct = salvo_res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert_eq!(ct, Some("application/json; charset=utf-8"));
    }

    #[test]
    fn render_response_bytes_body() {
        let mut unified = Response::new();
        unified.set_bytes_body(Bytes::from_static(b"\x00\x01\x02"));

        let mut salvo_res = SalvoResponse::new();
        render_response(unified, &mut salvo_res);

        match &salvo_res.body {
            salvo::http::body::ResBody::Once(bytes) => {
                assert_eq!(bytes, &b"\x00\x01\x02"[..]);
            }
            _ => panic!("Expected ResBody::Once"),
        }
    }
}
