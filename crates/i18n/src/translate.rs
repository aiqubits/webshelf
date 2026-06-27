/// translate! 宏定义
///
/// 语法：$field:ident: $en:literal => $zh:literal
///
/// 展开：
/// - pub struct Translations { pub $field: &'static str, ... }
/// - pub const EN: Translations { ... }
/// - pub const ZH: Translations { ... }
///
/// ⚠️ $crate 路径卫生：若此宏被外部 crate 导入，
/// 需将展开中的 Translations 改为 $crate::Translations，
/// 将 EN/ZH 改为 $crate::EN / $crate::ZH。
#[macro_export]
macro_rules! translate {
    ($(
        $(#[$attr:meta])*
        $field:ident: $en:literal => $zh:literal
    ),+ $(,)?) => {
        pub struct Translations {
            $($(#[$attr])* pub $field: &'static str),+
        }
        pub const EN: Translations = Translations {
            $($field: $en),+
        };
        pub const ZH: Translations = Translations {
            $($field: $zh),+
        };
        /// Auto-generated array of all (field_name, en_value, zh_value) tuples.
        /// Used by compile-time cross-validation tests to iterate fields without
        /// manual enumeration.
        ///
        /// Each entry: `(field_name_as_str, en_literal, zh_literal)`.
        /// Field attributes (e.g. `#[cfg(...)]`) are NOT replicated here — this
        /// array only mirrors the field list as written in the `translate!` call.
        #[cfg(test)]
        #[doc(hidden)]
        pub const ALL_TRANSLATION_FIELDS: &[(&str, &str, &str)] = &[
            $((stringify!($field), $en, $zh)),+
        ];
    }
}
