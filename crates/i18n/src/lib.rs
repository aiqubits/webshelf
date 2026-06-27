pub mod language;
pub mod tf;
pub mod translate;

#[cfg(feature = "dioxus")]
pub mod context;

mod translations;

pub use language::Language;
pub use tf::tf;
// translate! macro is re-exported via #[macro_export] and pub mod translate
pub use translations::{EN, Translations, ZH};

#[cfg(feature = "dioxus")]
pub use context::I18nContext;
