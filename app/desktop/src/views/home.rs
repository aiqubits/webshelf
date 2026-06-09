use dioxus::prelude::*;
use ui::Hero;

#[component]
pub fn Home() -> Element {
    rsx! {
        Hero {}

    }
}
