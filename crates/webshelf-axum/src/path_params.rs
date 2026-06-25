/// Extract path parameters by comparing route template with actual path.
#[cfg(test)]
pub(crate) fn extract_path_params(template: &str, actual: &str) -> Vec<(String, String)> {
    let template_segs: Vec<&str> = template.trim_matches('/').split('/').collect();
    let actual_segs: Vec<&str> = actual.trim_matches('/').split('/').collect();

    let mut params = Vec::new();
    for (t, a) in template_segs.iter().zip(actual_segs.iter()) {
        if let Some(name) = t.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let decoded = percent_encoding::percent_decode_str(a)
                .decode_utf8()
                .unwrap_or(std::borrow::Cow::Borrowed(*a));
            params.push((name.to_string(), decoded.to_string()));
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_params_basic() {
        let params = extract_path_params("/users/{id}", "/users/42");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "id");
        assert_eq!(params[0].1, "42");
    }

    #[test]
    fn test_extract_path_params_multiple() {
        let params = extract_path_params("/users/{userId}/posts/{postId}", "/users/1/posts/2");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "userId");
        assert_eq!(params[0].1, "1");
        assert_eq!(params[1].0, "postId");
        assert_eq!(params[1].1, "2");
    }

    #[test]
    fn test_extract_path_params_url_encoded() {
        let params = extract_path_params("/items/{name}", "/items/hello%20world");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "name");
        assert_eq!(params[0].1, "hello world");
    }

    #[test]
    fn test_extract_path_params_no_match() {
        let params = extract_path_params("/users/{id}", "/users/42/posts");
        assert_eq!(params.len(), 1); // zip truncates
    }

    #[test]
    fn test_extract_path_params_no_params() {
        let params = extract_path_params("/health", "/health");
        assert!(params.is_empty());
    }
}
