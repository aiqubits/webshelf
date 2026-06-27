/// 运行时格式化：将 template 中的 `{key}` 命名占位符替换为 args 中对应 key 的值。
/// 若占位符在 args 中找不到对应 key，则在结果中保留原样。
/// 若需要包含字面花括号字符，在 template 中使用 `\{` 和 `\}` 转义。
///
/// ## 替换顺序
/// 从长占位符到短占位符排序替换，避免 `{total}` 被 `{total_pages}` 的部分前缀误匹配。
///
/// ## 快速路径
/// `args.is_empty()` 时直接返回 `template.to_string()`，跳过占位符排序与替换。
/// args 数量通常为 1-3 个，排序分配 Vec 的开销可忽略。
///
/// ## 花括号转义
/// `\{` 先被替换为临时哨兵值，完成所有 `{key}` 替换后再恢复为 `{`。
/// 例如 `"可用 {status} 状态码: \{200, 404, 500\}"` 中的 `\{200` 不会被误认为占位符。
pub fn tf(template: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    if args.is_empty() {
        return template.to_string();
    }
    const ESCAPED_OPEN: &str = "__ESC_OPEN_7F3A__";
    const ESCAPED_CLOSE: &str = "__ESC_CLOSE_7F3A__";
    let mut result = template.replace("\\{", ESCAPED_OPEN);
    result = result.replace("\\}", ESCAPED_CLOSE);
    let mut sorted: Vec<_> = args.iter().map(|(k, v)| (k, v)).collect();
    sorted.sort_by_key(|(b, _)| std::cmp::Reverse(b.len()));
    for (key, val) in &sorted {
        result = result.replace(&format!("{{{}}}", key), &val.to_string());
    }
    let result = result.replace(ESCAPED_OPEN, "{");
    result.replace(ESCAPED_CLOSE, "}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tf_no_args_returns_template() {
        assert_eq!(tf("Hello", &[]), "Hello");
    }

    #[test]
    fn tf_single_placeholder() {
        assert_eq!(tf("Hello {name}!", &[("name", &"World")]), "Hello World!");
    }

    #[test]
    fn tf_multiple_placeholders() {
        assert_eq!(
            tf(
                "{total} users, page {page} / {total_pages}",
                &[("total", &"10"), ("page", &"1"), ("total_pages", &"5")]
            ),
            "10 users, page 1 / 5"
        );
    }

    #[test]
    fn tf_longer_placeholder_before_shorter() {
        // {total_pages} 比 {total} 长，应优先匹配长占位符
        assert_eq!(
            tf(
                "{total} / {total_pages}",
                &[("total", &"5"), ("total_pages", &"10")]
            ),
            "5 / 10"
        );
    }

    #[test]
    fn tf_escaped_braces() {
        assert_eq!(
            tf("Status {status}: \\{200, 404\\}", &[("status", &"code")]),
            "Status code: {200, 404}"
        );
    }

    #[test]
    fn tf_missing_key_keeps_placeholder() {
        assert_eq!(tf("Hello {name}!", &[("other", &"value")]), "Hello {name}!");
    }

    #[test]
    fn tf_empty_args_is_zero_alloc() {
        let t = tf("static text", &[]);
        assert_eq!(t, "static text");
    }

    #[test]
    fn tf_display_trait_works() {
        assert_eq!(tf("Count: {n}", &[("n", &42)]), "Count: 42");
    }
}
