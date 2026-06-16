use dioxus::prelude::*;

/// StatsCard —— Dashboard 顶部统计卡片。
///
/// 按 DESIGN.md §3.8 规格。包含图标块（语义 10% 背景）、标签、大数值与副文本。
#[component]
pub fn StatsCard(
    label: String,
    value: String,
    sub: String,
    /// Lucide 图标组件，例如 `rsx! { HeartPulse {} }`。
    icon: Element,
    #[props(default = StatsAccent::Indigo)] accent: StatsAccent,
    #[props(default)] value_color: StatsValueColor,
) -> Element {
    let icon_class = format!("ws-stats__icon ws-stats__icon--{}", accent.modifier());
    let value_class = format!(
        "ws-stats__value ws-stats__value--{}",
        value_color.modifier()
    );

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/styling/stats_card.css") }
        div { class: "ws-stats",
            div { class: "ws-stats__body",
                span { class: "ws-stats__label", "{label}" }
                span { class: value_class, "{value}" }
                span { class: "ws-stats__sub", "{sub}" }
            }
            div { class: icon_class, {icon} }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatsAccent {
    Emerald,
    #[default]
    Indigo,
    Purple,
    Amber,
}

impl StatsAccent {
    fn modifier(self) -> &'static str {
        match self {
            Self::Emerald => "emerald",
            Self::Indigo => "indigo",
            Self::Purple => "purple",
            Self::Amber => "amber",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatsValueColor {
    #[default]
    Default,
    Emerald,
    Amber,
}

impl StatsValueColor {
    fn modifier(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Emerald => "emerald",
            Self::Amber => "amber",
        }
    }
}
