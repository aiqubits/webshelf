use dioxus::prelude::*;
use dioxus_icons::lucide::Languages;

use crate::{I18nContext, Language};

/// 语言切换组件。三种变体：Header / Inline / Floating。
///
/// - **Header**：单按钮直接切换，显示当前语言，用于顶部导航栏。
/// - **Inline**：双按钮行内样式，用于 desktop/mobile Navbar 旁。
/// - **Floating**：固定定位浮动图标，仅用于 wasm32 未认证页面。
#[component]
pub fn LanguageSwitcher(variant: LanguageSwitcherVariant) -> Element {
    let mut ctx = use_context::<I18nContext>();
    let t = ctx.t();

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/language_switcher.css"),
        }

        match variant {
            LanguageSwitcherVariant::Header => rsx! {
                button {
                    class: "ls-header-btn",
                    title: if ctx.lang() == Language::En { t.lang_switcher_header_title_en } else { t.lang_switcher_header_title_zh },
                    onclick: move |_| {
                        let new = if ctx.lang() == Language::En { Language::Zh } else { Language::En };
                        ctx.set_lang(new);
                    },
                    Languages { class: "ls-header-icon" }
                    span { class: "ls-header-label",
                        if ctx.lang() == Language::En {
                            "EN"
                        } else {
                            "中文"
                        }
                    }
                }
            },
            LanguageSwitcherVariant::Inline => rsx! {
                div { class: "ls-inline",
                    button {
                        class: if ctx.lang() == Language::En { "ls-btn ls-btn--active" } else { "ls-btn" },
                        onclick: move |_| ctx.set_lang(Language::En),
                        "EN"
                    }
                    button {
                        class: if ctx.lang() == Language::Zh { "ls-btn ls-btn--active" } else { "ls-btn" },
                        onclick: move |_| ctx.set_lang(Language::Zh),
                        "中文"
                    }
                }
            },
            LanguageSwitcherVariant::Floating => rsx! {
                div { class: "ls-floating",
                    button {
                        class: "ls-floating__btn",
                        title: t.lang_switcher_floating_title,
                        onclick: move |_| {
                            let new = if ctx.lang() == Language::En { Language::Zh } else { Language::En };
                            ctx.set_lang(new);
                        },
                        Languages { class: "ls-floating__icon" }
                        span { class: "ls-floating__label",
                            if ctx.lang() == Language::En {
                                "中文"
                            } else {
                                "EN"
                            }
                        }
                    }
                }
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageSwitcherVariant {
    Header,
    Inline,
    Floating,
}
