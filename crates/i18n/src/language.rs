/// 支持的语言。新增变体时需同步更新 as_str() 和 t() 方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Zh,
    // TODO: ZhHant variant for zh-TW/zh-HK/zh-MO
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Zh => "zh",
        }
    }
}
