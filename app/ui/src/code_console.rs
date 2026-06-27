use dioxus::prelude::*;

use crate::{EN, I18nContext};

/// CodeConsole —— Dashboard 双栏区域左侧的「服务链路追踪监控」终端。
///
/// 按 DESIGN.md §3.12 规格。读取一组 `ConsoleLine`，自动滚动到底部。
#[component]
pub fn CodeConsole(lines: ReadSignal<Vec<ConsoleLine>>) -> Element {
    let i18n = try_use_context::<I18nContext>();
    let t = i18n.as_ref().map(|c| c.t()).unwrap_or(&EN);

    use_effect(move || {
        let line_count = lines.read().len();
        if line_count == 0 {
            return;
        }
        spawn(async move {
            let _ = document::eval(
                "const el = document.getElementById('ws-console-scroll'); \
                 if (el) { el.scrollTop = el.scrollHeight; }",
            )
            .await;
        });
    });

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/code_console.css"),
        }
        section { class: "ws-console",
            header { class: "ws-console__header",
                div { class: "ws-console__title-block",
                    h2 { class: "ws-console__title", {t.code_console_title} }
                    p { class: "ws-console__subtitle", {t.code_console_subtitle} }
                }
                span { class: "ws-console__live-tag", "live_stream" }
            }
            div {
                id: "ws-console-scroll",
                class: "ws-console__scroll no-scrollbar",
                {
                    let snapshot = lines.read().clone();
                    snapshot.into_iter().map(line_row)
                }
            }
        }
    }
}

fn line_row(line: ConsoleLine) -> Element {
    let dot_class = format!("ws-console__dot ws-console__dot--{}", line.kind.modifier());
    let method_class = format!(
        "ws-console__method ws-console__method--{}",
        line.kind.modifier()
    );
    rsx! {
        div { class: "ws-console__row",
            span { class: dot_class }
            if let Some(ts) = &line.timestamp {
                span { class: "ws-console__ts", "[{ts}]" }
            }
            if let Some(method) = &line.method {
                span { class: method_class, "{method}" }
            }
            span { class: "ws-console__path", "{line.path}" }
            if !line.status.is_empty() {
                span { class: "ws-console__status", "→ {line.status}" }
            }
        }
    }
}

/// 控制台一行。`method=None` 表示纯注释行（例如启动横幅）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleLine {
    pub timestamp: Option<String>,
    pub method: Option<String>,
    pub path: String,
    pub status: String,
    pub kind: ConsoleKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleKind {
    /// 绿色 — 2xx 成功
    Success,
    /// 紫色 — GET / 一般信息
    #[default]
    Info,
    /// 琥珀色 — PUT / 更新
    Amber,
    /// 玫瑰色 — DELETE / 错误
    Rose,
}

impl ConsoleKind {
    fn modifier(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Info => "info",
            Self::Amber => "amber",
            Self::Rose => "rose",
        }
    }
}
