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

use crate::auth::{AuthState, CurrentUser};
use crate::components::{
    HttpMethod, LogBus, LogEntry, LogKind, now_unix_ms, push_log_err, push_log_ok,
};

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
    // use_effect 异步任务版本号 —— 防止旧任务覆盖新任务的 checking 状态。
    // 工作原理同 users.rs 中的 list_version 模式。
    let mut health_version = use_signal(|| 0u64);
    let user_count_version = use_signal(|| 0u64);
    // 初始拉取健康信息 + 用户总数（如果是 admin）。
    // effect 追踪 auth.user 变化：当用户登录、登出、角色变更时自动刷新数据。
    {
        let client = auth.client.clone();
        let bus = log_bus;
        let nav_for_effect = nav;
        let checking_for_effect = checking;
        let auth_for_effect = auth.clone();
        let mut version_signal = health_version;
        let mut uc_version_signal = user_count_version;
        use_effect(move || {
            // 递增版本号并快照当前值，供异步任务完成后校验。
            let version = version_signal.with_mut(|v| {
                *v += 1;
                *v
            });
            let uc_version = uc_version_signal.with_mut(|v| {
                *v += 1;
                *v
            });
            let client = client.clone();
            let bus = bus;
            let auth_inner = auth_for_effect.clone();
            let nav_inner = nav_for_effect;
            let mut checking_inner = checking_for_effect;
            let version_check = version_signal;
            let uc_version_check = uc_version_signal;
            // 在 effect 闭包内实时读取 auth.user 以建立信号追踪，
            // 确保用户角色变更时触发重取。
            // system 角色同样具备 admin 能力，因此也拉取用户总数。
            let is_admin = is_admin_or_system(auth_for_effect.user.read().as_ref());
            spawn(async move {
                checking_inner.set(true);
                run_health_check(
                    client.clone(),
                    health,
                    latency_ms,
                    bus,
                    version,
                    version_check,
                )
                .await;
                if is_admin {
                    run_user_count(
                        client,
                        total_users,
                        bus,
                        auth_inner,
                        nav_inner,
                        uc_version,
                        uc_version_check,
                    )
                    .await;
                }
                // 版本校验：仅当本任务仍为最新版本时才修改信号状态，
                // 避免旧任务误覆盖新任务的 loading / 数据。
                if version_check() == version {
                    checking_inner.set(false);
                }
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

    // 是否为可查看管控用户数的管理员或系统角色。
    // 在渲染时实时读取以建立响应式追踪，登录/登出/角色变更时自动重渲染。
    let show_user_count = is_admin_or_system(auth.user.read().as_ref());

    let on_run_health = move |_| {
        let client = auth.client.clone();
        let bus = log_bus;
        let version = health_version.with_mut(|v| {
            *v += 1;
            *v
        });
        checking.set(true);
        spawn(async move {
            run_health_check(client, health, latency_ms, bus, version, health_version).await;
            // 版本号校验：仅最新任务才能重置 checking，
            // 旧任务不触碰信号，避免按钮提前释放。
            if health_version() == version {
                checking.set(false);
            }
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
                            "欢迎来到 WebShelf Rust 全栈脚手架系统 🚀"
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
                // 管控用户数：仅对 admin / system 角色可见。
                if show_user_count {
                    StatsCard {
                        label: "当前管控用户数".to_string(),
                        value: match users_snapshot {
                            Some(n) => n.to_string(),
                            None => "—".to_string(),
                        },
                        sub: "GET /api/users".to_string(),
                        icon: "fa-users",
                        accent: StatsAccent::Indigo,
                    }
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
    version: u64,
    health_version: Signal<u64>,
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
                version: hr.version,
                ok: true,
            });
            push_log_ok(bus, HttpMethod::Get, "/api/health");
        }
        Err(err) => {
            latency_ms.set(None);
            let (_, label) = error_summary(&err);
            health.set(HealthState {
                status_label: label.clone(),
                version: "—".to_string(),
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
            if crate::api::handle_unauth(&err, auth, nav, bus) {
                return;
            }
            push_log_err(bus, HttpMethod::Get, "/api/users", &err);
        }
    }
}

fn error_summary(err: &ClientError) -> (String, String) {
    match err {
        ClientError::Other(s, _) | ClientError::ServerError(s, _) => {
            (s.to_string(), format!("HTTP {s}"))
        }
        ClientError::Network(_) => ("NET".to_string(), "网络异常".to_string()),
        ClientError::RateLimited(_) => ("429".to_string(), "请求过于频繁".to_string()),
        ClientError::Config(_) => ("CFG".to_string(), "客户端配置异常".to_string()),
        ClientError::Deserialization(_) => ("JSON".to_string(), "响应数据解析失败".to_string()),
        // `ClientError` 标记为 #[non_exhaustive]，保留通配臂以兼容未来新增变体
        _ => (err.status_or_label(), "未知错误".to_string()),
    }
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

/// 判断当前用户是否具备 admin / system 权限（即可查看管控用户总数等统计信息）。
///
/// 与后端 `require_admin` 中间件的判定保持一致（[server/src/middlewares/auth.rs]）。
/// 普通 user 角色调用任何 admin-only 端点都会被 403 拒绝，因此这里必须一致。
fn is_admin_or_system(user: Option<&CurrentUser>) -> bool {
    matches!(
        user.map(|u| u.role.as_str()),
        Some("admin") | Some("system")
    )
}

#[cfg(test)]
mod tests {
    use super::is_admin_or_system;
    use crate::auth::CurrentUser;
    use uuid::Uuid;

    fn make_user(role: &str) -> CurrentUser {
        CurrentUser {
            id: Uuid::nil(),
            role: role.to_string(),
            name: String::new(),
            email: String::new(),
        }
    }

    #[test]
    fn admin_is_admin() {
        assert!(is_admin_or_system(Some(&make_user("admin"))));
    }

    #[test]
    fn system_is_admin() {
        assert!(is_admin_or_system(Some(&make_user("system"))));
    }

    #[test]
    fn user_is_not_admin() {
        assert!(!is_admin_or_system(Some(&make_user("user"))));
    }

    #[test]
    fn none_is_not_admin() {
        assert!(!is_admin_or_system(None));
    }

    #[test]
    fn guest_role_is_not_admin() {
        assert!(!is_admin_or_system(Some(&make_user("guest"))));
    }
}
