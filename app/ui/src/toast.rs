use dioxus::prelude::*;
use dioxus_icons::lucide::X;

/// 一条 toast 通知的展示数据。
///
/// 由 `LogBus` 写入，`<ToastStack>` 渲染。
/// UI 组件（`ui`）不直接依赖 `web::log_bus::LogEntry`，
/// 而是通过这套展示结构解耦，方便未来扩展（如推送通知）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToastEntry {
    /// 唯一 id，用于动画与 dismiss。
    pub id: u128,
    /// 决定 badge 颜色与显示文字；显示文字通过 `ToastMethod::as_str()` 获取。
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
    /// HTTP 方法显示文字，如 `"GET"`、`"POST"`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }

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
    Important,
}

impl ToastKind {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Success => "ws-toast--success",
            Self::Error => "ws-toast--error",
            Self::Important => "ws-toast--important",
        }
    }

    pub fn status_class(&self) -> &'static str {
        match self {
            Self::Success => "ws-toast__status--success",
            Self::Error | Self::Important => "ws-toast__status--error",
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
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/toast.css") }
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

    // 使用 use_resource 替代 use_effect + spawn：
    // use_resource 的 Future 在组件卸载时自动取消，避免旧 timer 在已销毁
    // 组件上调用 exiting.set() / on_dismiss.call() 导致未定义行为。
    //
    // 必须与 app/ui/assets/styling/toast.css 中 .ws-toast--exiting 的
    // transition-duration 保持同步（默认 300ms ease-in-out）。
    const EXIT_ANIM_MS: u64 = 300;
    use_resource(move || async move {
        // 自动 dismiss 延迟
        #[cfg(target_arch = "wasm32")]
        {
            gloo_timers::future::TimeoutFuture::new(auto_dismiss_ms.min(u32::MAX as u64) as u32)
                .await;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::time::sleep(std::time::Duration::from_millis(auto_dismiss_ms)).await;
        }
        exiting.set(true);

        // 退出动画持续时间
        #[cfg(target_arch = "wasm32")]
        {
            gloo_timers::future::TimeoutFuture::new(EXIT_ANIM_MS as u32).await;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::time::sleep(std::time::Duration::from_millis(EXIT_ANIM_MS)).await;
        }
        on_dismiss.call(id);
    });

    let class = if exiting() {
        format!(
            "ws-toast ws-toast--exiting {} {}",
            entry.kind.class(),
            entry.method_variant.class()
        )
    } else {
        format!(
            "ws-toast {} {}",
            entry.kind.class(),
            entry.method_variant.class()
        )
    };

    rsx! {
        div { class: "{class}",
            span { class: "ws-toast__method",
                span { class: "ws-toast__method-text", "{entry.method_variant.as_str()}" }
            }
            div { class: "ws-toast__body",
                div { class: "ws-toast__path", "{entry.path}" }
                div { class: "ws-toast__status {entry.kind.status_class()}",
                    "响应状态: {entry.status}"
                }
            }
            button {
                class: "ws-toast__close",
                onclick: move |_| on_dismiss.call(id),
                X {}
            }
        }
    }
}
