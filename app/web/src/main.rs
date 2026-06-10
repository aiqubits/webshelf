use dioxus::prelude::*;

use auth::AuthState;
use components::{AppShellLayout, LogBus};
use views::{Auth, Dashboard, NotFound, Users};

mod api;
mod auth;
mod components;
mod views;
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(AppShellLayout)]
        #[route("/")]
        Dashboard {},
        #[layout(crate::components::RequireAdmin)]
            #[route("/users")]
            Users {},
        #[end_layout]
    #[route("/auth")]
    Auth {},
    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(AuthState::new);
    let log_bus = use_context_provider(LogBus::new);

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        ToastLayer { bus: log_bus }
        Router::<Route> {}
    }
}

/// 全局 toast 层。订阅 `LogBus.entries` 的 `Signal` 并在收到新增项时
/// 渲染为 `ui::ToastEntry` 推送给 `ui::ToastStack`。
#[component]
fn ToastLayer(bus: LogBus) -> Element {
    let mut dismissed = use_signal(std::collections::HashSet::<u128>::new);

    let entries_signal = bus.entries;

    // 把 LogEntry 翻译成 ToastEntry（仅在 entries 变化时重算）。
    let toasts = use_memo(move || {
        let entries = entries_signal.read();
        entries
            .iter()
            .filter(|e| !dismissed.read().contains(&toast_id(e.id)))
            .map(toast_entry)
            .collect::<Vec<_>>()
    });

    rsx! {
        ui::ToastStack {
            entries: toasts(),
            on_dismiss: move |id| {
                dismissed.write().insert(id);
            },
        }
    }
}

/// LogEntry.id 是 Uuid；为避免与 ToastEntry 冲突，用 `Uuid::as_u128()` 取稳定哈希。
fn toast_id(uuid: uuid::Uuid) -> u128 {
    uuid.as_u128()
}

fn toast_entry(e: &components::LogEntry) -> ui::ToastEntry {
    use ui::{ToastEntry, ToastKind, ToastMethod};
    let method_variant = match e.method {
        components::HttpMethod::Get => ToastMethod::Get,
        components::HttpMethod::Post => ToastMethod::Post,
        components::HttpMethod::Put => ToastMethod::Put,
        components::HttpMethod::Delete => ToastMethod::Delete,
    };
    let kind = match e.kind {
        components::LogKind::Success => ToastKind::Success,
        components::LogKind::Error => ToastKind::Error,
        components::LogKind::Important => ToastKind::Important,
    };
    ToastEntry {
        id: toast_id(e.id),
        method: e.method.as_str().to_string(),
        method_variant,
        path: e.path.clone(),
        status: e.status.clone(),
        kind,
        created_at_ms: e.created_at_ms,
    }
}
