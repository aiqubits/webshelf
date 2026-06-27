use dioxus::prelude::*;
use dioxus_icons::lucide::Compass;

use ui::I18nContext;

use crate::Route;

#[component]
pub fn NotFound(route: Vec<String>) -> Element {
    let i18n = use_context::<I18nContext>();
    let t = i18n.t();
    let _ = route; // catch-all 段保留以满足 Routable trait
    rsx! {
        div { class: "ws-view-placeholder",
            Compass {}
            h1 { "404" }
            p { {t.not_found_page} }
            Link { to: Route::Dashboard {}, {t.not_found_back_to_dashboard} }
        }
    }
}
