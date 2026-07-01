use crate::translate;

// All translations loaded from external data file.
// Add new entries in translate_fields.rs — no changes needed here.
include!("translate_fields.rs");

// ============================================================
// Compile-time cross-validation tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Language;

    /// 品牌名 / 技术术语豁免列表——这些字段 EN=ZH 是正常预期，不触发断言。
    const EXEMPT: &[&str] = &[
        "sidebar_brand_title",             // 品牌名 WebShelf Admin 不翻译
        "sidebar_brand_subtitle",          // 品牌标语 Fullend System 不翻译
        "top_header_node_online",          // 技术术语 Node Online 不翻译
        "dashboard_middleware_active",     // 技术术语 Active 不翻译
        "login_copyright_sub",             // 品牌标语 Fullend Admin System 不翻译
        "auth_name_placeholder",           // 占位符 e.g., rust_master 中英相同
        "auth_email_placeholder_login",    // name@domain.com
        "auth_email_placeholder_register", // master@rust.org
        "auth_password_placeholder",       // ••••••••
        "app_shell_guest",                 // Guest 品牌名不翻译
        "dashboard_stats_users_sub",       // 技术术语 GET /api/users 不翻译
        "users_form_name_placeholder",     // 占位符 e.g., rust_master 中英相同
        "users_form_email_placeholder",    // 占位符 master@rust.org 中英相同
    ];

    /// 防止 EN/ZH 倒挂粘贴错误。
    ///
    /// 自动遍历 translate! 宏生成的 ALL_TRANSLATION_FIELDS，新增字段无需手动枚举。
    /// 品牌名 / 技术术语通过 EXEMPT 列表跳过。
    #[test]
    fn en_zh_values_not_identical() {
        for (name, en_val, zh_val) in ALL_TRANSLATION_FIELDS {
            if EXEMPT.contains(name) {
                continue;
            }
            assert!(
                en_val != zh_val,
                "EN/ZH 值相同可能为倒挂粘贴错误: {} (EN=`{}`, ZH=`{zh_val}`) — 若确属有意为之（如占位符/技术术语），请加入 EXEMPT 列表",
                name,
                en_val,
            );
        }
    }

    /// as_str() 覆盖所有 Language 变体
    #[test]
    fn language_as_str_covers_all_variants() {
        assert_eq!(Language::En.as_str(), "en");
        assert_eq!(Language::Zh.as_str(), "zh");
    }

    /// 确保 ALL_TRANSLATION_FIELDS 数组行数与 translate! 调用中的字段数一致。
    /// 这是一个防遗漏守卫——当新增或删除字段时，此计数值需同步更新。
    #[test]
    fn all_translation_fields_count() {
        let count = ALL_TRANSLATION_FIELDS.len();
        assert_eq!(
            count, 219,
            "ALL_TRANSLATION_FIELDS 计数 ({count}) 不符合预期 (219)。如果新增/删除了 translate! 字段，请同步更新此断言。"
        );
    }
}
