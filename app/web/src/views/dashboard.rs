//! Dashboard 视图（控制中心 /health）。
//!
//! Phase 4 —— Hero + 4 StatsCards + CodeConsole + 3 RouteCards 真实接线。

use client_api::ClientError;
use dioxus::prelude::dioxus_router::Navigator;
use dioxus::prelude::*;
use ui::{
    Button, CodeConsole, ConsoleKind, ConsoleLine, RouteCard, RouteMethod, StatsAccent, StatsCard,
    StatsValueColor,
};

use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, LogEntry, LogKind};

#[derive(Debug, Clone, Default)]
struct HealthState {
    status_label: String, // "—" / "UP (200 OK)" / "503 SERVICE_UNAVAILABLE"
    version: String,      // 服务版本，作为 sub
    ok: bool,
}

#[component]
pub fn Dashboard() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    let health = use_signal(|| HealthState {
        status_label: "—".to_string(),
        version: "尚未检测".to_string(),
        ok: false,
    });
    let latency_ms = use_signal(|| Option::<f64>::None);
    let total_users = use_signal(|| Option::<u64>::None);
    let mut checking = use_signal(|| false);

    // 初始拉取健康信息 + 用户总数（如果是 admin）。
    {
        let client = auth.client.clone();
        let bus = log_bus;
        let auth_for_effect = auth.clone();
        let nav_for_effect = nav;
        let checking_for_effect = checking;
        let is_admin = auth
            .user
            .read()
            .as_ref()
            .map(|u| u.role == "admin")
            .unwrap_or(false);
        use_effect(move || {
            let client = client.clone();
            let bus = bus;
            let auth_inner = auth_for_effect.clone();
            let nav_inner = nav_for_effect;
            let mut checking_inner = checking_for_effect;
            spawn(async move {
                checking_inner.set(true);
                run_health_check(client.clone(), health, latency_ms, bus).await;
                if is_admin {
                    run_user_count(client, total_users, bus, auth_inner, nav_inner).await;
                }
                checking_inner.set(false);
            });
        });
    }

    // 控制台行：从 LogBus 取并附加种子注释。
    let bus_entries = log_bus.entries;
    let console_lines = use_memo(move || build_console_lines(&bus_entries.read()));
    let console_lines_signal: ReadSignal<Vec<ConsoleLine>> = console_lines.into();

    let user_name = auth
        .user
        .read()
        .as_ref()
        .map(|u| u.name.clone())
        .unwrap_or_else(|| "WebShelf".to_string());

    let on_run_health = move |_| {
        let client = auth.client.clone();
        let bus = log_bus;
        checking.set(true);
        spawn(async move {
            run_health_check(client, health, latency_ms, bus).await;
            checking.set(false);
        });
    };

    let health_snapshot = health.read().clone();
    let latency_snapshot = *latency_ms.read();
    let users_snapshot = *total_users.read();

    rsx! {
        div { class: "ws-dashboard",
            section { class: "ws-hero",
                span { class: "ws-hero__orb ws-hero__orb--indigo" }
                span { class: "ws-hero__orb ws-hero__orb--pink" }
                div { class: "ws-hero__body",
                    div { class: "ws-hero__text",
                        h1 { class: "ws-hero__title",
                            "欢迎来到 WebShelf Rust 微服务脚手架系统 🚀"
                        }
                        p { class: "ws-hero__subtitle",
                            "你好 {user_name}！本控制台演示 axum + sea-orm + Dioxus 全栈链路，"
                            "点击右侧按钮发起一次真实的健康检查请求。"
                        }
                    }
                    div { class: "ws-hero__cta",
                        Button {
                            onclick: on_run_health,
                            disabled: checking(),
                            loading: checking(),
                            i { class: "fa-solid fa-heart-pulse ws-hero__cta-icon" }
                            "点此调用健康检查 (GET /health)"
                        }
                    }
                }
            }

            section { class: "ws-stats-grid",
                StatsCard {
                    label: "服务健康度".to_string(),
                    value: health_snapshot.status_label.clone(),
                    sub: format!("版本: {}", health_snapshot.version),
                    icon: "fa-heart-pulse",
                    accent: StatsAccent::Emerald,
                    value_color: if health_snapshot.ok { StatsValueColor::Emerald } else { StatsValueColor::Default },
                }
                StatsCard {
                    label: "当前管控用户数".to_string(),
                    value: match users_snapshot {
                        Some(n) => n.to_string(),
                        None => "—".to_string(),
                    },
                    sub: "GET /api/users (admin)".to_string(),
                    icon: "fa-users",
                    accent: StatsAccent::Indigo,
                }
                StatsCard {
                    label: "接口平均耗时".to_string(),
                    value: match latency_snapshot {
                        Some(ms) => format!("{ms:.2} ms"),
                        None => "—".to_string(),
                    },
                    sub: "/api/health 单次 RTT".to_string(),
                    icon: "fa-gauge-high",
                    accent: StatsAccent::Purple,
                }
                StatsCard {
                    label: "中间件拦截器状态".to_string(),
                    value: "Active".to_string(),
                    sub: "拦截器: require_admin".to_string(),
                    icon: "fa-shield-halved",
                    accent: StatsAccent::Amber,
                    value_color: StatsValueColor::Amber,
                }
            }

            section { class: "ws-dashboard__split",
                div { class: "ws-panel ws-panel--console",
                    CodeConsole { lines: console_lines_signal }
                }
                div { class: "ws-panel ws-panel--routes",
                    header { class: "ws-routes__header",
                        h2 { class: "ws-routes__title", "路由架构图" }
                        p { class: "ws-routes__subtitle", "axum::Router 的关键端点速查" }
                    }
                    div { class: "ws-routes__list",
                        RouteCard {
                            method: RouteMethod::Post,
                            path: "/api/public/auth/login".to_string(),
                            description: "颁发 JWT，公共端点".to_string(),
                        }
                        RouteCard {
                            method: RouteMethod::Get,
                            path: "/api/users".to_string(),
                            description: "分页列出用户，require_admin".to_string(),
                        }
                        RouteCard {
                            method: RouteMethod::Put,
                            path: "/api/users/{id}".to_string(),
                            description: "更新用户字段，require_admin".to_string(),
                        }
                    }
                }
            }
        }
    }
}

async fn run_health_check(
    client: client_api::Client,
    mut health: Signal<HealthState>,
    mut latency_ms: Signal<Option<f64>>,
    bus: LogBus,
) {
    let started = now_ms();
    let res = client.health_check().await;
    let elapsed = now_ms() - started;
    match res {
        Ok(hr) => {
            latency_ms.set(Some(elapsed));
            health.set(HealthState {
                status_label: format!("UP ({})", hr.status),
                version: hr.version,
                ok: true,
            });
            push_log(
                bus,
                HttpMethod::Get,
                "/api/health",
                "200 OK",
                LogKind::Success,
            );
        }
        Err(err) => {
            latency_ms.set(None);
            let (code, label) = error_summary(&err);
            health.set(HealthState {
                status_label: label.clone(),
                version: "—".to_string(),
                ok: false,
            });
            push_log(bus, HttpMethod::Get, "/api/health", &code, LogKind::Error);
        }
    }
}

async fn run_user_count(
    client: client_api::Client,
    mut total_users: Signal<Option<u64>>,
    bus: LogBus,
    auth: AuthState,
    nav: Navigator,
) {
    let res = client.list_users(1, 1).await;
    match res {
        Ok(page) => {
            total_users.set(Some(page.total));
            push_log(
                bus,
                HttpMethod::Get,
                "/api/users",
                "200 OK",
                LogKind::Success,
            );
        }
        Err(err) => {
            let (code, _) = error_summary(&err);
            total_users.set(None);
            push_log(bus, HttpMethod::Get, "/api/users", &code, LogKind::Error);
            let _ = crate::api::handle_unauth(&err, auth, nav);
        }
    }
}

fn error_summary(err: &ClientError) -> (String, String) {
    match err {
        ClientError::Other(s, _) | ClientError::ServerError(s, _) => {
            (s.to_string(), format!("HTTP {s}"))
        }
        ClientError::Network(_) => ("NET".to_string(), "网络异常".to_string()),
        _ => ("ERR".to_string(), "未知错误".to_string()),
    }
}

fn push_log(mut bus: LogBus, method: HttpMethod, path: &str, status: &str, kind: LogKind) {
    bus.push(method, path.to_string(), status.to_string(), kind);
}

fn build_console_lines(entries: &[LogEntry]) -> Vec<ConsoleLine> {
    let mut out = Vec::with_capacity(entries.len() + 2);
    out.push(ConsoleLine {
        timestamp: None,
        method: None,
        path: "// 监控引擎已就绪，等待 axum tower stack 路由事件…".to_string(),
        status: String::new(),
        kind: ConsoleKind::Info,
    });
    out.push(ConsoleLine {
        timestamp: None,
        method: None,
        path: "// 历史回放：以下为本次会话已捕获的真实请求 ↓".to_string(),
        status: String::new(),
        kind: ConsoleKind::Info,
    });
    for e in entries {
        let kind = match e.kind {
            LogKind::Success => match e.method {
                HttpMethod::Put => ConsoleKind::Amber,
                HttpMethod::Delete => ConsoleKind::Rose,
                _ => ConsoleKind::Success,
            },
            LogKind::Error => ConsoleKind::Rose,
            LogKind::Important => ConsoleKind::Amber,
        };
        out.push(ConsoleLine {
            timestamp: Some(format_ts(e.created_at_ms)),
            method: Some(e.method.as_str().to_string()),
            path: e.path.clone(),
            status: e.status.clone(),
            kind,
        });
    }
    out
}

fn format_ts(unix_ms: u64) -> String {
    use chrono::{TimeZone, Utc};
    let secs = (unix_ms / 1000) as i64;
    let ns = ((unix_ms % 1000) * 1_000_000) as u32;
    Utc.timestamp_opt(secs, ns)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".to_string())
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}
