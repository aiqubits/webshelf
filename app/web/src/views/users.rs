//! Users 管理视图（admin）。
//!
//! Phase 3 —— 完整接入 `client-api` 的 list / create / update / delete。
//! 通过 `LogBus` 写入 toast + console。

use chrono::{DateTime, Utc};
use client_api::UserResponse;
use dioxus::prelude::dioxus_router::Navigator;
use dioxus::prelude::*;
use dioxus_icons::lucide::{LoaderCircle, Pencil, Plus, ShieldHalf, Trash2, TriangleAlert};

use ui::{
    Align, Badge, BadgeVariant, Button, ButtonType, Column, DataTable, InputType, Modal, TextInput,
};

use crate::api::{ErrorContext, humanize_error};
use crate::auth::AuthState;
use crate::balance::{BALANCE_SCALE, format_balance};
use crate::components::{HttpMethod, LogBus, SearchSignal, push_log_err, push_log_ok};

#[derive(Debug, Clone)]
enum ListState {
    Loading,
    Loaded(Vec<UserResponse>),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalKind {
    None,
    Create,
    Edit,
    DeleteConfirm,
}

#[derive(Clone, Copy)]
struct UsersSignals {
    modal_kind: Signal<ModalKind>,
    editing_user: Signal<Option<UserResponse>>,
    deleting_user: Signal<Option<UserResponse>>,
    form_name: Signal<String>,
    form_email: Signal<String>,
    form_password: Signal<String>,
    form_role: Signal<String>,
    form_balance: Signal<String>,
    submitting: Signal<bool>,
    form_error: Signal<Option<String>>,
    list_version: Signal<u64>,
}

#[component]
pub fn Users() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();

    let list = use_signal(|| ListState::Loading);
    let list_version = use_signal(|| 0u64);
    let SearchSignal(search_query) = use_context::<SearchSignal>();

    let signals = UsersSignals {
        modal_kind: use_signal(|| ModalKind::None),
        editing_user: use_signal(|| Option::<UserResponse>::None),
        deleting_user: use_signal(|| Option::<UserResponse>::None),
        form_name: use_signal(String::new),
        form_email: use_signal(String::new),
        form_password: use_signal(String::new),
        form_role: use_signal(|| "user".to_string()),
        form_balance: use_signal(String::new),
        submitting: use_signal(|| false),
        form_error: use_signal(|| Option::<String>::None),
        list_version,
    };

    {
        let client = auth.client.clone();
        let bus = log_bus;
        let auth_for_effect = auth.clone();
        use_effect(move || {
            // 读取 list_version 以注册为 use_effect 的响应式依赖。
            // 值被主动丢弃，只有读取信号的副作用是必要的——
            // 缺少此行会导致 effect 在列表变更后不会重新执行。
            let _ = list_version.cloned();
            let client = client.clone();
            let mut list = list;
            let bus = bus;
            let auth_inner = auth_for_effect.clone();
            let version_check = list_version;
            spawn(async move {
                let version = version_check();
                list.set(ListState::Loading);
                let res = client.list_users(1, 20).await;
                // 丢弃过期响应：若 list_version 已递增，说明另一次获取已启动
                if version_check() != version {
                    return;
                }
                match res {
                    Ok(page) => {
                        push_log_ok(bus, HttpMethod::Get, "/api/users");
                        list.set(ListState::Loaded(page.items));
                    }
                    Err(err) => {
                        if crate::api::handle_unauth(&err, auth_inner, nav, bus) {
                            return;
                        }
                        push_log_err(bus, HttpMethod::Get, "/api/users", &err);
                        list.set(ListState::Error(humanize_error(
                            &err,
                            ErrorContext::UserManagement,
                        )));
                    }
                }
            });
        });
    }

    let list_snapshot = list.cloned();
    let search_text = search_query.cloned();
    let kind_snapshot = *signals.modal_kind.read();
    let form_error_snapshot = signals.form_error.read().clone();
    let submitting_snapshot = *signals.submitting.read();
    let editing_snapshot = signals.editing_user.read().clone();
    let deleting_snapshot = signals.deleting_user.read().clone();

    // Current user role and ID for permission checks
    let current_user = auth.user.read().as_ref().cloned();
    let current_role = current_user
        .as_ref()
        .map(|u| u.role.clone())
        .unwrap_or_default();
    let actor_is_system = current_user
        .as_ref()
        .map(|u| u.is_system())
        .unwrap_or(false);
    let actor_is_admin = current_user.as_ref().map(|u| u.is_admin()).unwrap_or(false);
    let current_user_id = current_user.map(|u| u.id).unwrap_or_default();

    let mut signals_for_open = signals;
    let open_create = move |_: MouseEvent| {
        signals_for_open.form_name.set(String::new());
        signals_for_open.form_email.set(String::new());
        signals_for_open.form_password.set(String::new());
        signals_for_open.form_role.set("user".to_string());
        signals_for_open.form_error.set(None);
        signals_for_open.editing_user.set(None);
        signals_for_open.modal_kind.set(ModalKind::Create);
    };

    rsx! {
        div { class: "ws-users",
            header { class: "ws-users__header",
                div { class: "ws-users__title-block",
                    h1 { class: "ws-users__title", "用户管理" }
                    p { class: "ws-users__subtitle",
                        "管理员可创建、编辑与移除系统用户实例"
                    }
                }
                div { class: "ws-users__header-actions",
                    span { class: "ws-users__guard-pill",
                        ShieldHalf {}
                        "require_admin 中间件保护区域"
                    }
                    Button { onclick: open_create,
                        Plus {}
                        "创建新用户 (POST)"
                    }
                }
            }

            {
                {
                    render_table(
                        list_snapshot,
                        search_text,
                        signals,
                        current_user_id,
                        actor_is_system,
                        actor_is_admin,
                    )
                }
            }

            {
                render_modal(
                    kind_snapshot,
                    form_error_snapshot,
                    submitting_snapshot,
                    editing_snapshot,
                    deleting_snapshot,
                    signals,
                    auth.client.clone(),
                    log_bus,
                    auth.clone(),
                    nav,
                    current_role,
                )
            }
        }
    }
}

fn render_table(
    list_snapshot: ListState,
    search_text: String,
    signals: UsersSignals,
    current_user_id: String,
    actor_is_system: bool,
    actor_is_admin: bool,
) -> Element {
    match list_snapshot {
        ListState::Loading => rsx! {
            div { class: "ws-users__status",
                LoaderCircle { class: "ws-btn__spinner" }
                "正在加载用户列表…"
            }
        },
        ListState::Error(msg) => rsx! {
            div { class: "ws-users__status ws-users__status--error",
                TriangleAlert {}
                "{msg}"
            }
        },
        ListState::Loaded(items) => {
            // 前端实时搜索过滤：按用户名或邮箱（不区分大小写）
            let filtered: Vec<UserResponse> = if search_text.is_empty() {
                items
            } else {
                let q = search_text.to_lowercase();
                items
                    .into_iter()
                    .filter(|u| {
                        u.name.to_lowercase().contains(&q) || u.email.to_lowercase().contains(&q)
                    })
                    .collect()
            };
            let columns = build_columns();
            let rows: Vec<Element> = filtered
                .into_iter()
                .map(|u| {
                    row_element(
                        u,
                        signals,
                        actor_is_system,
                        actor_is_admin,
                        current_user_id.clone(),
                    )
                })
                .collect();
            rsx! {
                DataTable {
                    columns,
                    rows,
                    empty: Some(rsx! { "暂无用户" }),
                }
            }
        }
    }
}

fn row_element(
    u: UserResponse,
    signals: UsersSignals,
    actor_is_system: bool,
    actor_is_admin: bool,
    current_user_id: String,
) -> Element {
    // 直接从原结构上提取展示字段，避免先 clone 再逐字段 clone 的冗余。
    let id_for_key = u.id.clone();
    let name = u.name.clone();
    let email = u.email.clone();
    let role = u.role.clone();
    let created = u.created_at;
    let balance = u.balance;

    let is_system = role == "system";
    let is_self = u.id == current_user_id;

    // Edit permission: system can edit all non-system; admin can only edit users
    let can_edit = (actor_is_system && !is_system) || (actor_is_admin && role == "user");
    // Delete permission: same as edit, but cannot delete self
    let can_delete = can_edit && !is_self;

    let u_for_edit = u.clone();
    let u_for_delete = u.clone();
    let mut s_edit = signals;
    let mut s_delete = signals;
    let edit_handler = move |_: MouseEvent| {
        s_edit.form_name.set(u_for_edit.name.clone());
        s_edit.form_email.set(u_for_edit.email.clone());
        s_edit.form_password.set(String::new());
        s_edit.form_role.set(u_for_edit.role.clone());
        s_edit.form_balance.set(String::new());
        s_edit.form_error.set(None);
        s_edit.editing_user.set(Some(u_for_edit.clone()));
        s_edit.modal_kind.set(ModalKind::Edit);
    };
    let delete_handler = move |_: MouseEvent| {
        s_delete.form_error.set(None);
        s_delete.deleting_user.set(Some(u_for_delete.clone()));
        s_delete.modal_kind.set(ModalKind::DeleteConfirm);
    };
    rsx! {
        tr { key: "{id_for_key}",
            td { class: "ws-table__mono", "{id_for_key}" }
            td { "{name}" }
            td { "{email}" }
            td {
                if role == "admin" {
                    Badge { variant: BadgeVariant::Admin, "管理员" }
                } else if role == "system" {
                    Badge { variant: BadgeVariant::Admin, "系统管理员" }
                } else {
                    Badge { variant: BadgeVariant::User, "普通用户" }
                }
            }
            td { class: "ws-table__mono ws-table__align--right", "{format_balance(balance)}" }
            td { class: "ws-table__mono", "{format_dt(&created)}" }
            td {
                if is_system {
                    div { class: "ws-table__row-actions",
                        span { class: "ws-table__protected",
                            ShieldHalf {}
                            " 受保护"
                        }
                    }
                } else if !can_edit && !can_delete {
                    div { class: "ws-table__row-actions",
                        span { class: "ws-table__protected",
                            ShieldHalf {}
                            " 权限不足"
                        }
                    }
                } else {
                    div { class: "ws-table__row-actions",
                        if can_edit {
                            button {
                                class: "ws-table__action",
                                title: "编辑",
                                onclick: edit_handler,
                                Pencil {}
                            }
                        }
                        if can_delete {
                            button {
                                class: "ws-table__action ws-table__action--danger",
                                title: "删除",
                                onclick: delete_handler,
                                Trash2 {}
                            }
                        }
                    }
                }
            }
        }
    }
}

fn close_all(mut signals: UsersSignals) {
    signals.modal_kind.set(ModalKind::None);
    signals.editing_user.set(None);
    signals.deleting_user.set(None);
    signals.form_name.set(String::new());
    signals.form_email.set(String::new());
    signals.form_password.set(String::new());
    signals.form_role.set("user".to_string());
    signals.form_balance.set(String::new());
    signals.submitting.set(false);
    signals.form_error.set(None);
}

/// Create a balance adjustment handler closure.
///
/// `multiplier` controls direction: `1` = increase, `-1` = decrease.
fn make_adjust_handler(
    mut signals: UsersSignals,
    client: client_api::Client,
    log_bus: LogBus,
    auth: AuthState,
    nav: Navigator,
    multiplier: i64,
) -> impl FnMut(MouseEvent) {
    move |_: MouseEvent| {
        if *signals.submitting.read() {
            return;
        }
        let Some(u) = signals.editing_user.cloned() else {
            return;
        };
        let target_id = u.id.clone();
        let text = signals.form_balance.cloned();
        if text.trim().is_empty() {
            signals.form_error.set(Some("请输入调整金额".into()));
            return;
        }
        let display: f64 = match text.parse() {
            Ok(v) => v,
            Err(_) => {
                signals
                    .form_error
                    .set(Some("金额格式无效，请输入数字 (如 0.50)".into()));
                return;
            }
        };
        if display <= 0.0 {
            signals.form_error.set(Some("金额必须大于 0".into()));
            return;
        }
        // Reject non-finite values (NaN, infinity) that can bypass the >0 check
        if !display.is_finite() {
            signals
                .form_error
                .set(Some("金额格式无效，请输入数字 (如 0.50)".into()));
            return;
        }
        // Protect against i64 overflow: display * BALANCE_SCALE must fit in i64
        if display > 1_000_000.0 {
            signals
                .form_error
                .set(Some("金额超出允许范围，最大 1,000,000".into()));
            return;
        }
        let stored = (display * BALANCE_SCALE as f64).round() as i64 * multiplier;
        signals.submitting.set(true);
        signals.form_error.set(None);
        let mut s_async = signals;
        let c_async = client.clone();
        let b_async = log_bus;
        let a_async = auth.clone();
        spawn(async move {
            let res = c_async.adjust_balance(target_id.clone(), stored).await;
            s_async.submitting.set(false);
            match res {
                Ok(resp) => {
                    push_log_ok(
                        b_async,
                        HttpMethod::Post,
                        &format!("/api/users/{}/balance/adjust", target_id),
                    );
                    if let Some(ref mut u) = *s_async.editing_user.write() {
                        u.balance = resp.balance;
                    }
                    s_async.form_balance.set(String::new());
                    s_async.list_version.with_mut(|v| *v += 1);
                }
                Err(err) => {
                    if crate::api::handle_unauth(&err, a_async, nav, b_async) {
                        return;
                    }
                    push_log_err(
                        b_async,
                        HttpMethod::Post,
                        &format!("/api/users/{}/balance/adjust", target_id),
                        &err,
                    );
                    s_async
                        .form_error
                        .set(Some(humanize_error(&err, ErrorContext::UserManagement)));
                }
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn render_modal(
    kind: ModalKind,
    form_error: Option<String>,
    submitting: bool,
    editing: Option<UserResponse>,
    deleting: Option<UserResponse>,
    signals: UsersSignals,
    client: client_api::Client,
    log_bus: LogBus,
    auth: AuthState,
    nav: Navigator,
    current_role: String,
) -> Element {
    if kind == ModalKind::None {
        return VNode::empty();
    }

    if kind == ModalKind::DeleteConfirm {
        return render_delete_modal(
            form_error,
            submitting,
            deleting,
            signals,
            client,
            log_bus,
            auth,
            nav,
            current_role,
        );
    }

    render_form_modal(
        kind,
        form_error,
        submitting,
        editing,
        signals,
        client,
        log_bus,
        auth,
        nav,
        current_role,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_delete_modal(
    form_error: Option<String>,
    submitting: bool,
    deleting: Option<UserResponse>,
    signals: UsersSignals,
    client: client_api::Client,
    log_bus: LogBus,
    auth: AuthState,
    nav: Navigator,
    current_role: String,
) -> Element {
    let on_close = move |_: MouseEvent| close_all(signals);
    let on_cancel = move |_: MouseEvent| close_all(signals);
    let mut signals_for_submit = signals;
    let on_confirm = move |_: MouseEvent| {
        // 防止快速双击触发两次 spawn
        if *signals_for_submit.submitting.read() {
            return;
        }
        let Some(u) = signals_for_submit.deleting_user.cloned() else {
            signals_for_submit
                .form_error
                .set(Some("未选择目标用户".into()));
            return;
        };
        // 防御性检查：系统用户不可删除（后端同样保护）
        if u.role == "system" {
            signals_for_submit
                .form_error
                .set(Some("系统管理员不可删除".into()));
            return;
        }
        // 防御性检查：admin 不能删除 admin 或 system（后端同样保护）
        if current_role == "admin" && u.role != "user" {
            signals_for_submit
                .form_error
                .set(Some("权限不足：管理员只能删除普通用户".into()));
            return;
        }
        let target_id = u.id.clone();
        signals_for_submit.submitting.set(true);
        signals_for_submit.form_error.set(None);
        let client_async = client.clone();
        let bus_async = log_bus;
        let mut s_async = signals_for_submit;
        let auth_async = auth.clone();
        spawn(async move {
            let res = client_async.delete_user(target_id.clone()).await;
            if res.is_ok() {
                push_log_ok(
                    bus_async,
                    HttpMethod::Delete,
                    &format!("/api/users/{target_id}"),
                );
            }
            s_async.submitting.set(false);
            match res {
                Ok(_) => {
                    // 仅当模态框仍为 DeleteConfirm 时才关闭，避免旧异步任务关闭新打开的模态框。
                    if *s_async.modal_kind.read() == ModalKind::DeleteConfirm {
                        s_async.modal_kind.set(ModalKind::None);
                        s_async.deleting_user.set(None);
                        s_async.form_error.set(None);
                    }
                    s_async.list_version.with_mut(|v| *v += 1);
                }
                Err(err) => {
                    if crate::api::handle_unauth(&err, auth_async, nav, bus_async) {
                        return;
                    }
                    push_log_err(
                        bus_async,
                        HttpMethod::Delete,
                        &format!("/api/users/{target_id}"),
                        &err,
                    );
                    s_async
                        .form_error
                        .set(Some(humanize_error(&err, ErrorContext::UserManagement)));
                }
            }
        });
    };
    rsx! {
        Modal {
            title: "确认删除",
            on_close,
            open: true,
            disable_backdrop: submitting,
            div { class: "ws-form-stack",
                if let Some(err) = form_error.as_ref() {
                    p { class: "ws-form-error", "{err}" }
                }
                if let Some(u) = deleting {
                    p { class: "ws-delete-msg",
                        "确定要删除用户 "
                        strong { "{u.name} " }
                        "("
                        span { class: "ws-table__mono", "{u.id}" }
                        ") 吗？此操作不可撤销。"
                    }
                } else {
                    p { class: "ws-delete-msg", "未选择目标用户" }
                }
                div { class: "ws-delete-actions",
                    Button {
                        button_type: ButtonType::Button,
                        disabled: submitting,
                        onclick: on_cancel,
                        "取消"
                    }
                    Button {
                        button_type: ButtonType::Submit,
                        disabled: submitting,
                        loading: submitting,
                        onclick: on_confirm,
                        "确认删除 (DELETE)"
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_form_modal(
    kind: ModalKind,
    form_error: Option<String>,
    submitting: bool,
    editing: Option<UserResponse>,
    signals: UsersSignals,
    client: client_api::Client,
    log_bus: LogBus,
    auth: AuthState,
    nav: Navigator,
    current_role: String,
) -> Element {
    let title = if kind == ModalKind::Create {
        "创建新用户"
    } else {
        "编辑用户节点"
    };
    let password_required = kind == ModalKind::Create;
    let submit_label = if kind == ModalKind::Create {
        "创建实体 (POST)"
    } else {
        "保存实体 (PUT)"
    };
    let password_placeholder = if kind == ModalKind::Create {
        "≥8 字符，含大小写 / 数字 / 标点"
    } else {
        "留空表示不修改"
    };

    let on_close = move |_: MouseEvent| close_all(signals);
    let mut signals_for_role = signals;
    let pick_admin = move |_: MouseEvent| signals_for_role.form_role.set("admin".to_string());
    let pick_user = move |_: MouseEvent| signals_for_role.form_role.set("user".to_string());

    let editing_for_submit = editing.clone();
    let mut signals_for_submit = signals;
    // Admin 不能修改 role，clone 一份供闭包使用（current_role 在之后 JSX 中还有引用）
    let current_role_for_submit = current_role.clone();
    // Clone auth and client before on_submit moves them; originals used by balance adjustment closures
    let auth_for_submit = auth.clone();
    let client_for_submit = client.clone();
    let on_submit = move |_: MouseEvent| {
        // 防止快速双击触发两次 spawn
        if *signals_for_submit.submitting.read() {
            return;
        }
        let name = signals_for_submit.form_name.cloned();
        let email = signals_for_submit.form_email.cloned().to_lowercase();
        let password = signals_for_submit.form_password.cloned();
        let role = signals_for_submit.form_role.cloned();
        let editing_id = editing_for_submit.as_ref().map(|u| u.id.clone());
        let kind_now = kind;
        // 防御性检查：系统用户不可编辑（后端同样保护，UI 按钮已隐藏）
        if kind_now == ModalKind::Edit
            && editing_for_submit
                .as_ref()
                .map(|u| u.role == "system")
                .unwrap_or(false)
        {
            signals_for_submit
                .form_error
                .set(Some("系统管理员不可编辑".into()));
            return;
        }
        // 同步校验：避免空字段浪费网络请求（Create 与 Edit 模式均需校验 name/email）
        if name.trim().is_empty() {
            signals_for_submit.form_error.set(Some("用户名为空".into()));
            return;
        }
        if email.trim().is_empty() {
            signals_for_submit
                .form_error
                .set(Some("邮箱不能为空".into()));
            return;
        }
        if kind_now == ModalKind::Create && password.is_empty() {
            signals_for_submit
                .form_error
                .set(Some("密码不能为空".into()));
            return;
        }
        let client_async = client_for_submit.clone();
        let bus_async = log_bus;
        let mut s_async = signals_for_submit;
        let auth_async = auth_for_submit.clone();
        signals_for_submit.submitting.set(true);
        signals_for_submit.form_error.set(None);
        // Admin 不能修改 role，不发送该字段；仅 system 可以
        // 在 spawn 外部计算布尔值以避免 String 被移入 async move 块
        let actor_is_sys = current_role_for_submit == "system";
        spawn(async move {
            let res = match kind_now {
                ModalKind::Create => {
                    let create_role = if actor_is_sys {
                        Some(role.clone())
                    } else {
                        None
                    };
                    let r = client_async
                        .create_user(&email, &password, &name, create_role)
                        .await;
                    if r.is_ok() {
                        push_log_ok(bus_async, HttpMethod::Post, "/api/users");
                    }
                    r.map(|_| ())
                }
                ModalKind::Edit => {
                    let Some(ref id) = editing_id else {
                        s_async.form_error.set(Some("缺少用户 ID".into()));
                        s_async.submitting.set(false);
                        return;
                    };
                    let edit_role = if actor_is_sys {
                        Some(role.clone())
                    } else {
                        None
                    };
                    let r = client_async
                        .update_user(
                            id.clone(),
                            Some(email.clone()),
                            Some(name.clone()),
                            edit_role,
                        )
                        .await;
                    if r.is_ok() {
                        push_log_ok(bus_async, HttpMethod::Put, &format!("/api/users/{}", id));
                    }
                    r.map(|_| ())
                }
                _ => unreachable!(),
            };
            s_async.submitting.set(false);
            match res {
                Ok(_) => {
                    // 仅当模态框类型未变更时才关闭，避免旧异步任务关闭新打开的模态框。
                    if *s_async.modal_kind.read() == kind_now {
                        s_async.modal_kind.set(ModalKind::None);
                        s_async.editing_user.set(None);
                        s_async.form_name.set(String::new());
                        s_async.form_email.set(String::new());
                        s_async.form_password.set(String::new());
                        s_async.form_role.set("user".to_string());
                        s_async.form_error.set(None);
                    }
                    s_async.list_version.with_mut(|v| *v += 1);
                }
                Err(err) => {
                    if crate::api::handle_unauth(&err, auth_async, nav, bus_async) {
                        return;
                    }
                    // 根据操作类型重建日志路径，避免在 inner match 中提前写日志导致双 toast
                    let log_method = if kind_now == ModalKind::Create {
                        HttpMethod::Post
                    } else {
                        HttpMethod::Put
                    };
                    let log_path = if kind_now == ModalKind::Create {
                        "/api/users".to_string()
                    } else {
                        format!("/api/users/{}", editing_id.unwrap_or_default())
                    };
                    push_log_err(bus_async, log_method, &log_path, &err);
                    s_async
                        .form_error
                        .set(Some(humanize_error(&err, ErrorContext::UserManagement)));
                }
            }
        });
    };

    let role_now = signals.form_role.cloned();

    // Balance adjustment: visible in Edit mode when the actor has permission
    let can_adjust = editing
        .as_ref()
        .map(|u| {
            (current_role == "system" && u.role != "system")
                || (current_role == "admin" && u.role == "user")
        })
        .unwrap_or(false);

    let on_increase = make_adjust_handler(signals, client.clone(), log_bus, auth.clone(), nav, 1);
    let on_decrease = make_adjust_handler(signals, client.clone(), log_bus, auth.clone(), nav, -1);

    rsx! {
        Modal {
            title: title.to_string(),
            on_close,
            open: true,
            disable_backdrop: submitting,
            div { class: "ws-form-stack",
                if let Some(err) = form_error.as_ref() {
                    p { class: "ws-form-error", "{err}" }
                }
                TextInput {
                    label: "账户名".to_string(),
                    placeholder: Some("e.g., rust_master".to_string()),
                    value: signals.form_name,
                    required: true,
                    disabled: submitting,
                    name: Some("name".to_string()),
                }
                TextInput {
                    label: "邮箱地址".to_string(),
                    placeholder: Some("master@rust.org".to_string()),
                    value: signals.form_email,
                    input_type: InputType::Email,
                    required: true,
                    disabled: submitting,
                    name: Some("email".to_string()),
                }
                if kind == ModalKind::Create {
                    TextInput {
                        label: "安全密码".to_string(),
                        placeholder: Some(password_placeholder.to_string()),
                        value: signals.form_password,
                        input_type: InputType::Password,
                        required: password_required,
                        disabled: submitting,
                        name: Some("password".to_string()),
                        hint: Some(
                            "密码需 ≥8 字符，包含大小写字母、数字和 ASCII 标点"
                                .to_string(),
                        ),
                    }
                }
                if kind == ModalKind::Edit && current_role == "system" {
                    div { class: "ws-form-field",
                        label { class: "ws-form-label", "系统授权标签" }
                        div { class: "ws-form-pill-group",
                            button {
                                r#type: "button",
                                class: if role_now == "admin" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_admin,
                                "管理员"
                            }
                            button {
                                r#type: "button",
                                class: if role_now == "user" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_user,
                                "普通用户"
                            }
                        }
                    }
                }
                // 创建模式：仅 system 角色可设置初始角色
                if kind == ModalKind::Create && current_role == "system" {
                    div { class: "ws-form-field",
                        label { class: "ws-form-label", "初始授权标签" }
                        div { class: "ws-form-pill-group",
                            button {
                                r#type: "button",
                                class: if role_now == "admin" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_admin,
                                "管理员"
                            }
                            button {
                                r#type: "button",
                                class: if role_now == "user" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_user,
                                "普通用户"
                            }
                        }
                    }
                }
                Button {
                    button_type: ButtonType::Submit,
                    full_width: true,
                    disabled: submitting,
                    loading: submitting,
                    onclick: on_submit,
                    "{submit_label}"
                }
                // 余额调整：编辑模式且当前用户有权限时显示
                if kind == ModalKind::Edit && can_adjust {
                    if let Some(u) = editing.as_ref() {
                        div { class: "ws-form-field",
                            hr {}
                            div { class: "ws-form-label", "余额调整" }
                            p { class: "ws-form-description",
                                "当前余额: "
                                strong { "{format_balance(u.balance)}" }
                            }
                            TextInput {
                                label: "调整金额".to_string(),
                                placeholder: Some("例如 0.50".to_string()),
                                value: signals.form_balance,
                                input_type: InputType::Text,
                                required: false,
                                disabled: submitting,
                            }
                            div { class: "ws-form-pill-group",
                                button {
                                    class: "ws-btn ws-btn--primary ws-btn--pill",
                                    r#type: "button",
                                    disabled: submitting,
                                    onclick: on_increase,
                                    "增加余额"
                                }
                                button {
                                    class: "ws-btn ws-btn--primary ws-btn--pill",
                                    r#type: "button",
                                    disabled: submitting,
                                    onclick: on_decrease,
                                    "减少余额"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn build_columns() -> Vec<Column> {
    vec![
        Column::new("ID").width("w-32").align(Align::Left),
        Column::new("账户身份").align(Align::Left),
        Column::new("安全邮箱").align(Align::Left),
        Column::new("授权标签").align(Align::Left),
        Column::new("余额").width("w-24").align(Align::Right),
        Column::new("实例孵化时间").width("w-40").align(Align::Left),
        Column::new("操作").width("w-44").align(Align::Center),
    ]
}

fn format_dt(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}
