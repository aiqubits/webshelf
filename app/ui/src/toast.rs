use dioxus::prelude::*;

/// 一条 toast 通知的展示数据。
///
/// 由 `LogBus` 写入，`<ToastStack>` 渲染。
/// UI 组件（`ui`）不直接依赖 `web::log_bus::LogEntry`，
/// 而是通过这套展示结构解耦，方便未来扩展（如推送通知）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToastEntry {
    /// 唯一 id，用于动画与 dismiss。
    pub id: u128,
    /// HTTP 方法字面值（"GET" / "POST" / "PUT" / "DELETE"）。
    pub method: String,
    /// 决定 badge 颜色。
    pub method_variant: ToastMethod,
    /// API 路径，如 `/api/users`。
    pub path: String,
    /// 状态文字，如 `200 OK`、`401 Unauthorized`。
    pub status: String,
    /// 决定状态文字颜色与 dismiss 时机。
    pub kind: ToastKind,
    /// 创建时间（Unix 毫秒）。用于入场动画与自动销毁。
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl ToastMethod {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Get => "ws-toast__method--get",
            Self::Post => "ws-toast__method--post",
            Self::Put => "ws-toast__method--put",
            Self::Delete => "ws-toast__method--delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
    Important,
}

impl ToastKind {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Success => "ws-toast--success",
            Self::Error => "ws-toast--error",
            Self::Info => "ws-toast--info",
            Self::Important => "ws-toast--important",
        }
    }

    pub fn status_class(&self) -> &'static str {
        match self {
            Self::Success => "ws-toast__status--success",
            Self::Error | Self::Important => "ws-toast__status--error",
            Self::Info => "ws-toast__status--info",
        }
    }
}

/// Toast 容器组件 —— 订阅 `entries` 并渲染 toast 列表。
///
/// 应挂在 App 根级别（不在 `AppShellLayout` 内），这样登录页等
/// 不在 shell 内的视图也能看到通知。
#[component]
pub fn ToastStack(
    entries: ReadSignal<Vec<ToastEntry>>,
    #[props(default = 3500)] auto_dismiss_ms: u64,
    on_dismiss: EventHandler<u128>,
) -> Element {
    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/toast.css"),
        }
        div { class: "ws-toast-container",
            for entry in entries.cloned() {
                Toast {
                    key: "{entry.id}",
                    entry,
                    auto_dismiss_ms,
                    on_dismiss,
                }
            }
        }
    }
}

#[component]
fn Toast(entry: ToastEntry, auto_dismiss_ms: u64, on_dismiss: EventHandler<u128>) -> Element {
    let id = entry.id;
    let mut exiting = use_signal(|| false);

    use_effect(move || {
        // 自动 dismiss —— 先切 exiting 让 CSS 退出动画播完，再回调 on_dismiss
        let entry_id = id;
        let dismiss_delay_ms = auto_dismiss_ms;
        let exit_anim_ms: u64 = 300;
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::future::TimeoutFuture::new(dismiss_delay_ms as u32).await;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::time::sleep(std::time::Duration::from_millis(dismiss_delay_ms)).await;
            }
            exiting.set(true);
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::future::TimeoutFuture::new(exit_anim_ms as u32).await;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::time::sleep(std::time::Duration::from_millis(exit_anim_ms)).await;
            }
            on_dismiss.call(entry_id);
        });
    });

    let class = format!(
        "ws-toast {} {}",
        entry.kind.class(),
        entry.method_variant.class()
    );
    if exiting() {
        rsx! {
            div { class: "ws-toast ws-toast--exiting {entry.kind.class()} {entry.method_variant.class()}",
                span { class: "ws-toast__method",
                    span { class: "ws-toast__method-text", "{entry.method}" }
                }
                div { class: "ws-toast__body",
                    div { class: "ws-toast__path", "{entry.path}" }
                    div { class: "ws-toast__status {entry.kind.status_class()}", "响应状态: {entry.status}" }
                }
                button {
                    class: "ws-toast__close",
                    onclick: move |_| on_dismiss.call(id),
                    i { class: "fa-solid fa-xmark" }
                }
            }
        }
    } else {
        rsx! {
            div { class: "{class}",
                span { class: "ws-toast__method",
                    span { class: "ws-toast__method-text", "{entry.method}" }
                }
                div { class: "ws-toast__body",
                    div { class: "ws-toast__path", "{entry.path}" }
                    div { class: "ws-toast__status {entry.kind.status_class()}", "响应状态: {entry.status}" }
                }
                button {
                    class: "ws-toast__close",
                    onclick: move |_| on_dismiss.call(id),
                    i { class: "fa-solid fa-xmark" }
                }
            }
        }
    }
}
