use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        div {
            h1 { "Hello, WebShelf!" }
            p { "Welcome to the WebShelf frontend." }
        }
    }
}
