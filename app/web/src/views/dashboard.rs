//! Dashboard 视图（控制中心 /health）。
//!
//! Phase 4 —— Hero + 4 StatsCards + CodeConsole + 3 RouteCards 真实接线。

use client_api::ClientError;
use dioxus::prelude::dioxus_router::Navigator;
use dioxus::prelude::*;
use dioxus_icons::lucide::{Gauge, HeartPulse, ShieldHalf, Users as UsersIcon};
use ui::{
    Button, CodeConsole, ConsoleKind, ConsoleLine, I18nContext, RouteCard, RouteMethod,
    StatsAccent, StatsCard, StatsValueColor, Translations, tf,
};

use crate::auth::AuthState;
use crate::components::{
    HttpMethod, LogBus, LogEntry, LogKind, SearchSignal, now_unix_ms, push_log_err, push_log_ok,
};

#[derive(Debug, Clone, Default)]
struct HealthState {
    status_label: String,    // "—" / "UP (200 OK)" / "503 SERVICE_UNAVAILABLE"
    version: Option<String>, // None = 尚未检测, Some(v) = 版本字符串
    ok: bool,
}

#[component]
pub fn Dashboard() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();
    let SearchSignal(search_query) = use_context::<SearchSignal>();
    let i18n = use_context::<I18nContext>();
    let t = i18n.t();

    let health = use_signal(|| HealthState {
        status_label: "—".to_string(),
        version: None, // None = not yet checked; label applied at render time via t at that point
        ok: false,
    });
    let latency_ms = use_signal(|| Option::<f64>::None);
    let total_users = use_signal(|| Option::<u64>::None);
    let mut checking = use_signal(|| false);
    // 异步任务版本号 —— 防止旧任务覆盖新任务的状态。
    // 初始值 0，每次点击按钮时递增。
    let mut health_version = use_signal(|| 0u64);
    let mut user_count_version = use_signal(|| 0u64);

    // 控制台行：从 LogBus 取并附加种子注释，支持搜索框实时过滤。
    let bus_entries = log_bus.entries;
    let console_lines = use_memo(move || {
        let t = i18n.t();
        let entries = bus_entries.read();
        let q = search_query.read().clone();
        if q.is_empty() {
            build_console_lines(&entries, t)
        } else {
            let q_lower = q.to_lowercase();
            let filtered: Vec<LogEntry> = entries
                .iter()
                .filter(|e| {
                    e.path.to_lowercase().contains(&q_lower)
                        || e.method.as_str().to_lowercase().contains(&q_lower)
                        || e.status.to_lowercase().contains(&q_lower)
                })
                .cloned()
                .collect();
            build_console_lines(&filtered, t)
        }
    });
    let console_lines_signal: ReadSignal<Vec<ConsoleLine>> = console_lines.into();

    let user_name = auth
        .user
        .read()
        .as_ref()
        .map(|u| u.name.clone())
        .unwrap_or_else(|| "WebShelf".to_string());

    // 是否为可查看管控用户数的管理员或系统角色。
    // 在渲染时实时读取以建立响应式追踪，登录/登出/角色变更时自动重渲染。
    let show_user_count = auth
        .user
        .read()
        .as_ref()
        .map(|u| u.is_admin())
        .unwrap_or(false);

    let on_run_health = move |_| {
        let client = auth.client.clone();
        let bus = log_bus;
        let version = health_version.with_mut(|v| {
            *v += 1;
            *v
        });
        let uc_version = user_count_version.with_mut(|v| {
            *v += 1;
            *v
        });
        let is_admin = auth
            .user
            .read()
            .as_ref()
            .map(|u| u.is_admin())
            .unwrap_or(false);
        // 克隆供 spawn 使用，保持外层闭包为 FnMut（onclick 要求）
        let auth_for_spawn = auth.clone();
        let nav_for_spawn = nav;
        checking.set(true);
        spawn(async move {
            run_health_check(
                client.clone(),
                health,
                latency_ms,
                bus,
                version,
                health_version,
                i18n,
            )
            .await;
            if is_admin {
                run_user_count(
                    client,
                    total_users,
                    bus,
                    auth_for_spawn,
                    nav_for_spawn,
                    uc_version,
                    user_count_version,
                )
                .await;
            }
            if health_version() == version {
                checking.set(false);
            }
        });
    };

    let health_snapshot = health.read().clone();
    let latency_snapshot = *latency_ms.read();
    let users_snapshot = *total_users.read();
    let search_text = search_query.cloned();

    const API_ROUTES: &[(RouteMethod, &str, &str)] = &[
        // ── 公共端点（无需认证）──
        (RouteMethod::Get, "/api/health", "健康检查，公共端点"),
        // ── 认证端点（无需认证）──
        (
            RouteMethod::Post,
            "/api/public/auth/login",
            "登录，颁发 JWT",
        ),
        (RouteMethod::Post, "/api/public/auth/register", "注册新账户"),
        (
            RouteMethod::Post,
            "/api/public/auth/verify-email",
            "提交 6 位邮箱验证码",
        ),
        (
            RouteMethod::Post,
            "/api/public/auth/resend-code",
            "重新发送验证码（60s 冷却）",
        ),
        (
            RouteMethod::Post,
            "/api/public/auth/forgot-password",
            "申请密码重置邮件",
        ),
        (
            RouteMethod::Post,
            "/api/public/auth/reset-password",
            "提交验证码并重置密码",
        ),
        // ── 自助端点（任意已认证用户）──
        (RouteMethod::Get, "/api/users/me", "获取当前用户资料"),
        (
            RouteMethod::Post,
            "/api/users/me/password",
            "修改当前用户密码",
        ),
        // ── 管理端点（require_admin）──
        (RouteMethod::Get, "/api/users", "分页列出用户"),
        (RouteMethod::Post, "/api/users", "创建新用户"),
        (RouteMethod::Get, "/api/users/{id}", "获取单个用户"),
        (RouteMethod::Put, "/api/users/{id}", "更新用户字段"),
        (RouteMethod::Delete, "/api/users/{id}", "删除用户"),
    ];

    rsx! {
        div { class: "ws-dashboard",
            section { class: "ws-hero",
                span { class: "ws-hero__orb ws-hero__orb--indigo" }
                span { class: "ws-hero__orb ws-hero__orb--pink" }
                div { class: "ws-hero__body",
                    div { class: "ws-hero__text",
                        h1 { class: "ws-hero__title", {t.dashboard_title} }
                        p { class: "ws-hero__subtitle",
                            {tf(t.dashboard_hello_user, &[("name", &user_name)])}
                            " "
                            {t.dashboard_click_health_hint}
                        }
                    }
                    div { class: "ws-hero__cta",
                        Button {
                            onclick: on_run_health,
                            disabled: checking(),
                            loading: checking(),
                            HeartPulse { class: "ws-hero__cta-icon" }
                            {t.dashboard_call_health_btn}
                        }
                    }
                }
            }

            section { class: "ws-stats-grid",
                StatsCard {
                    label: t.dashboard_stats_health_label.to_string(),
                    value: health_snapshot.status_label.clone(),
                    sub: format!(
                        "{}: {}",
                        t.dashboard_version_label,
                        health_snapshot.version.as_deref().unwrap_or(t.dashboard_not_yet_checked),
                    ),
                    icon: rsx! {
                        HeartPulse {}
                    },
                    accent: StatsAccent::Emerald,
                    value_color: if health_snapshot.ok { StatsValueColor::Emerald } else { StatsValueColor::Default },
                }
                // 管控用户数：仅对 admin / system 角色可见。
                if show_user_count {
                    StatsCard {
                        label: t.dashboard_stats_users_label.to_string(),
                        value: match users_snapshot {
                            Some(n) => n.to_string(),
                            None => "—".to_string(),
                        },
                        sub: t.dashboard_stats_users_sub.to_string(),
                        icon: rsx! {
                            UsersIcon {}
                        },
                        accent: StatsAccent::Indigo,
                    }
                }
                StatsCard {
                    label: t.dashboard_stats_latency_label.to_string(),
                    value: match latency_snapshot {
                        Some(ms) => format!("{ms:.2} ms"),
                        None => "—".to_string(),
                    },
                    sub: t.dashboard_stats_latency_sub.to_string(),
                    icon: rsx! {
                        Gauge {}
                    },
                    accent: StatsAccent::Purple,
                }
                StatsCard {
                    label: t.dashboard_stats_middleware_label.to_string(),
                    value: t.dashboard_middleware_active.to_string(),
                    sub: t.dashboard_stats_middleware_sub.to_string(),
                    icon: rsx! {
                        ShieldHalf {}
                    },
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
                        h2 { class: "ws-routes__title", {t.dashboard_routes_title} }
                        p { class: "ws-routes__subtitle", {t.dashboard_routes_subtitle} }
                    }
                    div { class: "ws-routes__list",
                        {
                            let filtered_routes: Vec<_> = if search_text.is_empty() {
                                API_ROUTES.to_vec()
                            } else {
                                let q = search_text.to_lowercase();
                                API_ROUTES
                                    .iter()
                                    .filter(|(_, path, desc)| {
                                        path.to_lowercase().contains(&q)
                                            || desc.to_lowercase().contains(&q)
                                    })
                                    .copied()
                                    .collect()
                            };
                            if filtered_routes.is_empty() && !search_text.is_empty() {
                                rsx! {
                                    p { class: "ws-routes__empty", {t.dashboard_no_match} }
                                }
                            } else {
                                rsx! {
                                    for (method , path , desc) in filtered_routes.into_iter() {
                                        RouteCard { method, path: path.to_string(), description: desc.to_string() }
                                    }
                                }
                            }
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
    version: u64,
    health_version: Signal<u64>,
    ctx: I18nContext,
) {
    let started = now_unix_ms();
    let res = client.health_check().await;
    let elapsed = (now_unix_ms() - started) as f64;
    // 版本号校验：有更新的 health check 已启动时，放弃本次写入
    if health_version() != version {
        return;
    }
    match res {
        Ok(hr) => {
            latency_ms.set(Some(elapsed));
            health.set(HealthState {
                status_label: format!("UP ({})", hr.status),
                version: Some(hr.version),
                ok: true,
            });
            push_log_ok(bus, HttpMethod::Get, "/api/health");
        }
        Err(err) => {
            latency_ms.set(None);
            let t = ctx.t();
            let (_, label) = error_summary(&err, t);
            health.set(HealthState {
                status_label: label.clone(),
                version: Some("—".to_string()),
                ok: false,
            });
            push_log_err(bus, HttpMethod::Get, "/api/health", &err);
        }
    }
}

async fn run_user_count(
    client: client_api::Client,
    mut total_users: Signal<Option<u64>>,
    bus: LogBus,
    auth: AuthState,
    nav: Navigator,
    version: u64,
    user_count_version: Signal<u64>,
) {
    let res = client.list_users(1, 1).await;
    if user_count_version() != version {
        return;
    }
    match res {
        Ok(page) => {
            total_users.set(Some(page.total));
            push_log_ok(bus, HttpMethod::Get, "/api/users");
        }
        Err(err) => {
            total_users.set(None);
            if crate::api::handle_unauth(&err, auth, nav, bus).await {
                return;
            }
            push_log_err(bus, HttpMethod::Get, "/api/users", &err);
        }
    }
}

fn error_summary(err: &ClientError, t: &Translations) -> (String, String) {
    match err {
        ClientError::Other(s, _) | ClientError::ServerError(s, _) => {
            (s.to_string(), format!("HTTP {s}"))
        }
        ClientError::Network(_) => ("NET".to_string(), t.dashboard_error_network.to_string()),
        ClientError::RateLimited(_) => (
            "429".to_string(),
            t.dashboard_error_rate_limited.to_string(),
        ),
        ClientError::Config(_) => ("CFG".to_string(), t.dashboard_error_config.to_string()),
        ClientError::Deserialization(_) => (
            "JSON".to_string(),
            t.dashboard_error_deserialize.to_string(),
        ),
        // `ClientError` 标记为 #[non_exhaustive]，保留通配臂以兼容未来新增变体
        _ => (err.status_or_label(), t.dashboard_error_unknown.to_string()),
    }
}

fn build_console_lines(entries: &[LogEntry], t: &Translations) -> Vec<ConsoleLine> {
    let mut out = Vec::with_capacity(entries.len() + 2);
    out.push(ConsoleLine {
        timestamp: None,
        method: None,
        path: t.dashboard_console_line_ready.to_string(),
        status: String::new(),
        kind: ConsoleKind::Info,
    });
    out.push(ConsoleLine {
        timestamp: None,
        method: None,
        path: t.dashboard_console_line_replay.to_string(),
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
