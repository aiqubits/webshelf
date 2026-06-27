use crate::{EN, Language, Translations, ZH};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub struct I18nContext {
    lang: Signal<Language>,
}

impl I18nContext {
    pub fn new(lang: Language) -> Self {
        Self {
            lang: Signal::new(lang),
        }
    }

    /// 读取当前语言。Dioxus 自动追踪此 signal。
    pub fn lang(&self) -> Language {
        (self.lang)()
    }

    /// 返回当前语言对应的 Translations 常量。
    /// 在 rsx! 模板中引用 `{i18n.t().xxx}` 时，Dioxus 自动追踪 signal，
    /// 语言切换后组件自动重渲染，无需手动 `.to_string()`。
    pub fn t(&self) -> &'static Translations {
        match self.lang() {
            Language::En => &EN,
            Language::Zh => &ZH,
        }
    }

    /// 切换语言。
    /// wasm32 下自动写入 localStorage（key: "webshelf_lang_v1"）。
    pub fn set_lang(&mut self, lang: Language) {
        self.lang.set(lang);
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Err(e) = storage.set_item("webshelf_lang_v1", lang.as_str()) {
                        // 类型由 web_sys::console::warn_2 签名中的 &JsValue 推断
                        let msg = "webshelf i18n: localStorage write failed:".into();
                        web_sys::console::warn_2(&msg, &e);
                    }
                }
            }
        }
    }
}
