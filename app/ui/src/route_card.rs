use dioxus::prelude::*;

/// RouteCard —— Dashboard 路由架构图中的单条路由展示。
///
/// 按 DESIGN.md §3.13 规格。三种语义色与 HTTP 方法对应。
#[component]
pub fn RouteCard(method: RouteMethod, path: String, description: String) -> Element {
    let card_class = format!("ws-route ws-route--{}", method.card_modifier());
    let badge_class = format!(
        "ws-route__badge ws-route__badge--{}",
        method.badge_modifier()
    );

    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/route_card.css"),
        }
        div { class: card_class,
            span { class: badge_class, "{method.label()}" }
            div { class: "ws-route__body",
                span { class: "ws-route__path", "{path}" }
                span { class: "ws-route__desc", "{description}" }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMethod {
    /// POST → indigo 卡片背景 + indigo badge
    Post,
    /// GET → purple 卡片背景 + purple badge
    Get,
    /// PUT → pink 卡片背景 + rose badge
    Put,
}

impl RouteMethod {
    fn label(self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Get => "GET",
            Self::Put => "PUT",
        }
    }

    fn card_modifier(self) -> &'static str {
        match self {
            Self::Post => "indigo",
            Self::Get => "purple",
            Self::Put => "pink",
        }
    }

    fn badge_modifier(self) -> &'static str {
        match self {
            Self::Post => "indigo",
            Self::Get => "purple",
            Self::Put => "rose",
        }
    }
}
