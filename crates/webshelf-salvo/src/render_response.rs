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
