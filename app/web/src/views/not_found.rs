use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn NotFound(route: Vec<String>) -> Element {
    let _ = route; // catch-all 段保留以满足 Routable trait
    rsx! {
        div { class: "ws-view-placeholder",
            i { class: "fa-solid fa-compass" }
            h1 { "404" }
            p { "你访问的页面不存在。" }
            Link { to: Route::Dashboard {}, "返回控制中心" }
        }
    }
}
