use dioxus::prelude::*;

use auth::AuthState;
use components::{AppShellLayout, LogBus, RequireAuth};
use views::{
    Auth, Dashboard, ForgotPassword, LoginLanding, NotFound, ResetPassword, Settings, Users,
    VerifyEmail,
};

mod api;
mod auth;
mod balance;
mod components;
mod views;
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    // ─ 公开路由（无需认证）──
    #[route("/")]
    LoginLanding {},
    #[route("/auth")]
    Auth {},
    #[route("/forgot-password")]
    ForgotPassword {},
    #[route("/reset-password")]
    #[route("/reset-password/:email")]
    ResetPassword { email: Option<String> },
    #[route("/verify-email/:email")]
    VerifyEmail { email: String },

    // ── 受保护路由（需登录）──
    #[layout(RequireAuth)]
        #[layout(AppShellLayout)]
            #[route("/dashboard")]
            Dashboard {},
            #[route("/settings")]
            Settings {},
            #[layout(crate::components::RequireAdmin)]
                #[route("/users")]
                Users {},
            #[end_layout]
        #[end_layout]
    #[end_layout]

    // ── 404 兜底 ──
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
    let auth = use_context::<AuthState>();

    // 应用启动时一次性恢复 localStorage 中的会话并拉取真实用户资料。
    // 必须放在路由挂载之前——这样无论首屏路由是 /、/users、/settings 还是 /auth，
    // 都已经有正确的 auth.user 状态，避免 "记住登录" 后直接刷新受保护页面
    // 表现为未登录的 BUG（Issue B1）。
    let mut once_flag = use_signal(|| false);
    use_effect(move || {
        if !*once_flag.read() {
            once_flag.set(true);
            // 闭包需要 FnMut (可能被多次调用) 而 spawn 需 FnOnce (会移动 auth)；
            // 在闭包内 clone 后再 move 进 spawn，避免与 use_effect 的 FnMut 冲突。
            let mut auth_clone = auth.clone();
            spawn(async move {
                auth_clone.restore_from_storage_async().await;
            });
        }
    });

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        ui::GlobalStyles {}
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        ToastLayer { bus: log_bus }
        Router::<Route> {}
    }
}

/// 全局 toast 层。订阅 `LogBus.entries` 的 `Signal` 并在收到新增项时
/// 渲染为 `ui::ToastEntry` 推送给 `ui::ToastStack`。
#[component]
fn ToastLayer(bus: LogBus) -> Element {
    let mut dismissed = use_signal(std::collections::HashSet::<u64>::new);

    let entries_signal = bus.entries;

    // 把 LogEntry 翻译成 ToastEntry（仅在 entries 变化时重算）。
    // use_memo 保持纯计算 —— 写入 dismissed 的副作用在独立 use_effect 中处理，
    // 避免 memo 因修改自身追踪的信号而触发额外重算。
    let toasts = use_memo(move || {
        let entries = entries_signal.read();
        entries
            .iter()
            .filter(|e| !dismissed.read().contains(&e.id))
            .map(toast_entry)
            .collect::<Vec<_>>()
    });
    let toasts_signal: ReadSignal<Vec<ui::ToastEntry>> = toasts.into();

    // 清理 dismissed 中已从 LogBus 淘汰的过期 ID，防止内存无限增长。
    let mut dismissed_for_cleanup = dismissed;
    use_effect(move || {
        let active_ids: std::collections::HashSet<u64> =
            bus.entries.read().iter().map(|e| e.id).collect();
        dismissed_for_cleanup
            .write()
            .retain(|id| active_ids.contains(id));
    });

    rsx! {
        ui::ToastStack {
            entries: toasts_signal,
            on_dismiss: move |id| {
                dismissed.write().insert(id);
            },
        }
    }
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
        id: e.id,
        method_variant,
        path: e.path.clone(),
        status: e.status.clone(),
        kind,
        created_at_ms: e.created_at_ms,
    }
}
