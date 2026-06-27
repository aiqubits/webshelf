//! Users 管理视图（admin）。
//!
//! Phase 3 —— 完整接入 `client-api` 的 list / create / update / delete。
//! 通过 `LogBus` 写入 toast + console。

use std::cell::Cell;
use std::rc::Rc;

use chrono::{DateTime, Utc};
use client_api::UserResponse;
use dioxus::prelude::dioxus_router::Navigator;
use dioxus::prelude::*;
use dioxus_icons::lucide::{LoaderCircle, Pencil, Plus, ShieldHalf, Trash2, TriangleAlert};

use ui::{
    Align, Badge, BadgeVariant, Button, ButtonType, Column, DataTable, I18nContext, InputType,
    Modal, TextInput, Translations, tf,
};

use crate::api::{ErrorContext, humanize_error};
use crate::auth::AuthState;
use crate::balance::{BALANCE_SCALE, format_balance};
use crate::components::{
    ConfirmDialog, HttpMethod, LogBus, SearchSignal, push_log_err, push_log_ok,
};

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
    page: Signal<u64>,
    per_page: Signal<u64>,
}

/// 分页状态（快照值 + 信号），用于 `render_table` 传参。
#[derive(Clone, Copy)]
struct PaginationState {
    page: u64,
    per_page: u64,
    total: u64,
    total_pages: u64,
    page_signal: Signal<u64>,
}

#[component]
pub fn Users() -> Element {
    let auth = use_context::<AuthState>();
    let log_bus = use_context::<LogBus>();
    let nav = use_navigator();
    let i18n = use_context::<I18nContext>();
    let t = i18n.t();

    let list = use_signal(|| ListState::Loading);
    let list_version = use_signal(|| 0u64);
    let page = use_signal(|| 1u64);
    let per_page = use_signal(|| 20u64);
    let total = use_signal(|| 0u64);
    let total_pages = use_signal(|| 0u64);
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
        page,
        per_page,
    };

    // 搜索变更时自动重置到第 1 页，避免翻到后面的页码后搜索结果为空。
    // 使用 use_hook + Rc<Cell<bool>> 持有非响应式的 "首次运行" 标记，
    // 避免写入信号触发 effect 自身重入（原 search_reset_done 方案会导致多余的重执行）。
    {
        let search_for_reset = search_query;
        let mut page_for_reset = page;
        let first_run: Rc<Cell<bool>> = use_hook(|| Rc::new(Cell::new(true))).clone();
        use_effect(move || {
            let _ = search_for_reset.read();
            if first_run.get() {
                first_run.set(false);
            } else {
                page_for_reset.set(1);
            }
        });
    }

    {
        let client = auth.client.clone();
        let bus = log_bus;
        let auth_for_effect = auth.clone();
        use_effect(move || {
            // 读取响应式依赖：list_version / page / per_page 变更时重新拉取
            let _ = list_version();
            let current_page = page();
            let current_per_page = per_page();
            let client = client.clone();
            let mut list = list;
            let bus = bus;
            let auth_inner = auth_for_effect.clone();
            let version_check = list_version;
            let mut total_signal = total;
            let mut total_pages_signal = total_pages;
            spawn(async move {
                let version = version_check();
                list.set(ListState::Loading);
                let res = client.list_users(current_page, current_per_page).await;
                // 丢弃过期响应：list_version / page / per_page 任一变化均视为过期
                if version_check() != version
                    || page() != current_page
                    || per_page() != current_per_page
                {
                    return;
                }
                match res {
                    Ok(page_data) => {
                        push_log_ok(bus, HttpMethod::Get, "/api/users");
                        total_signal.set(page_data.total);
                        total_pages_signal.set(page_data.total_pages);
                        list.set(ListState::Loaded(page_data.items));
                    }
                    Err(err) => {
                        if crate::api::handle_unauth(&err, auth_inner, nav, bus).await {
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
    let page_val = page();
    let per_page_val = per_page();
    let total_val = total();
    let total_pages_val = total_pages();
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
                    h1 { class: "ws-users__title", "{t.users_title}" }
                    p { class: "ws-users__subtitle",
                        "{t.users_subtitle}"
                    }
                }
                div { class: "ws-users__header-actions",
                    span { class: "ws-users__guard-pill",
                        ShieldHalf {}
                        "{t.users_guard_pill}"
                    }
                    Button { onclick: open_create,
                        Plus {}
                        "{t.users_create_btn}"
                    }
                }
            }

            {
                {
                    render_table(
                        t,
                        list_snapshot,
                        search_text,
                        signals,
                        current_user_id,
                        actor_is_system,
                        actor_is_admin,
                        PaginationState {
                            page: page_val,
                            per_page: per_page_val,
                            total: total_val,
                            total_pages: total_pages_val,
                            page_signal: page,
                        },
                    )
                }
            }

            {
                render_modal(
                    t,
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

#[allow(clippy::too_many_arguments)]
fn render_table(
    t: &'static Translations,
    list_snapshot: ListState,
    search_text: String,
    mut signals: UsersSignals,
    current_user_id: String,
    actor_is_system: bool,
    actor_is_admin: bool,
    pagination: PaginationState,
) -> Element {
    match list_snapshot {
        ListState::Loading => rsx! {
            div { class: "ws-users__status",
                LoaderCircle { class: "ws-btn__spinner" }
                "{t.users_loading}"
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
            let columns = build_columns(t);
            let rows: Vec<Element> = filtered
                .into_iter()
                .map(|u| {
                    row_element(
                        t,
                        u,
                        signals,
                        actor_is_system,
                        actor_is_admin,
                        current_user_id.clone(),
                    )
                })
                .collect();

            let PaginationState {
                page,
                per_page,
                total,
                total_pages,
                page_signal,
            } = pagination;
            let has_prev = page > 1;
            let has_next = page < total_pages;

            let mut prev_sig = page_signal;
            let on_prev = move |_: MouseEvent| {
                prev_sig.set(page.saturating_sub(1).max(1));
            };
            let mut next_sig = page_signal;
            let on_next = move |_: MouseEvent| {
                next_sig.set((page + 1).min(total_pages));
            };
            let pagination_info = if total_pages == 0 {
                tf(t.users_count_simple, &[("total", &total.to_string())])
            } else {
                tf(
                    t.users_count_info,
                    &[
                        ("total", &total.to_string()),
                        ("page", &page.to_string()),
                        ("total_pages", &total_pages.to_string()),
                    ],
                )
            };

            rsx! {
                div { class: "ws-users__table-wrapper",
                    DataTable {
                        columns,
                        rows,
                        empty: Some(rsx! { "{t.users_empty}" }),
                    }
                    div { class: "ws-pagination",
                        div { class: "ws-pagination__info", "{pagination_info}" }
                        div { class: "ws-pagination__controls",
                            button {
                                class: "ws-pagination__btn",
                                disabled: !has_prev,
                                onclick: on_prev,
                                "{t.users_prev_page}"
                            }
                            button {
                                class: "ws-pagination__btn",
                                disabled: !has_next,
                                onclick: on_next,
                                "{t.users_next_page}"
                            }
                            div { class: "ws-pagination__per-page",
                                span { "{t.users_per_page_label}" }
                                select {
                                    class: "ws-pagination__select",
                                    value: "{per_page}",
                                    onchange: move |evt| {
                                        if let Ok(v) = evt.value().parse::<u64>() {
                                            let v = v.clamp(1, 100);
                                            signals.per_page.set(v);
                                            signals.page.set(1);
                                        }
                                    },
                                    option { value: "20", "20" }
                                    option { value: "50", "50" }
                                    option { value: "100", "100" }
                                }
                                span { "{t.users_per_page_unit}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn row_element(
    t: &'static Translations,
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
            td {
                span {
                    class: "ws-table__name-cell",
                    title: "ID: {id_for_key}",
                    "data-id": "{id_for_key}",
                    "{name}"
                }
            }
            td { "{email}" }
            td {
                if role == "admin" {
                    Badge { variant: BadgeVariant::Admin, "{t.users_badge_admin}" }
                } else if role == "system" {
                    Badge { variant: BadgeVariant::Admin, "{t.users_badge_system}" }
                } else {
                    Badge { variant: BadgeVariant::User, "{t.users_badge_user}" }
                }
            }
            td { class: "ws-table__mono ws-table__align--right", "{format_balance(balance)}" }
            td { class: "ws-table__mono", "{format_dt(&created)}" }
            td {
                if is_system {
                    div { class: "ws-table__row-actions",
                        span { class: "ws-table__protected",
                            ShieldHalf {}
                            "{t.users_protected_label}"
                        }
                    }
                } else if !can_edit && !can_delete {
                    div { class: "ws-table__row-actions",
                        span { class: "ws-table__protected",
                            ShieldHalf {}
                            "{t.users_no_permission}"
                        }
                    }
                } else {
                    div { class: "ws-table__row-actions",
                        if can_edit {
                            button {
                                class: "ws-table__action",
                                title: "{t.users_edit_title}",
                                onclick: edit_handler,
                                Pencil {}
                            }
                        }
                        if can_delete {
                            button {
                                class: "ws-table__action ws-table__action--danger",
                                title: "{t.users_delete_title}",
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
    t: &'static Translations,
) -> impl FnMut(MouseEvent) + 'static {
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
            signals
                .form_error
                .set(Some(t.users_adjust_empty.to_string()));
            return;
        }
        let display: f64 = match text.parse() {
            Ok(v) => v,
            Err(_) => {
                signals
                    .form_error
                    .set(Some(t.users_adjust_invalid.to_string()));
                return;
            }
        };
        if display <= 0.0 {
            signals
                .form_error
                .set(Some(t.users_adjust_positive.to_string()));
            return;
        }
        // Reject non-finite values (NaN, infinity) that can bypass the >0 check
        if !display.is_finite() {
            signals
                .form_error
                .set(Some(t.users_adjust_invalid.to_string()));
            return;
        }
        // Protect against i64 overflow: display * BALANCE_SCALE must fit in i64
        if display > 1_000_000.0 {
            signals
                .form_error
                .set(Some(t.users_adjust_overflow.to_string()));
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
                    if crate::api::handle_unauth(&err, a_async, nav, b_async).await {
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
    t: &'static Translations,
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
        return render_delete_confirm(
            t,
            kind,
            deleting,
            submitting,
            signals,
            client,
            log_bus,
            auth,
            nav,
            current_role,
        );
    }

    render_form_modal(
        t,
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
fn render_delete_confirm(
    t: &'static Translations,
    kind: ModalKind,
    deleting: Option<UserResponse>,
    submitting: bool,
    signals: UsersSignals,
    client: client_api::Client,
    log_bus: LogBus,
    auth: AuthState,
    nav: Navigator,
    current_role: String,
) -> Element {
    let open = kind == ModalKind::DeleteConfirm;
    let message = deleting
        .as_ref()
        .map(|u| {
            tf(
                t.users_confirm_delete_msg,
                &[("name", &u.name), ("id", &u.id)],
            )
        })
        .unwrap_or_else(|| t.users_no_target.to_string());
    let confirm_delete_title = t.users_confirm_delete_title.to_string();

    let on_cancel = move |_: MouseEvent| close_all(signals);
    let mut s_async = signals;
    let c_async = client;
    let b_async = log_bus;
    let a_async = auth;
    let role = current_role;
    let on_confirm = move |_: MouseEvent| {
        if *s_async.submitting.read() {
            return;
        }
        let Some(u) = s_async.deleting_user.cloned() else {
            return;
        };
        // 防御性检查：系统用户不可删除（后端同样保护）
        if u.role == "system" {
            return;
        }
        // 防御性检查：admin 不能删除 admin 或 system（后端同样保护）
        if role == "admin" && u.role != "user" {
            return;
        }
        let target_id = u.id.clone();
        s_async.submitting.set(true);
        let client_async = c_async.clone();
        let bus_async = b_async;
        let mut s_inner = s_async;
        let auth_async = a_async.clone();
        spawn(async move {
            let res = client_async.delete_user(target_id.clone()).await;
            if res.is_ok() {
                push_log_ok(
                    bus_async,
                    HttpMethod::Delete,
                    &format!("/api/users/{target_id}"),
                );
            }
            s_inner.submitting.set(false);
            match res {
                Ok(_) => {
                    // 仅当模态框仍为 DeleteConfirm 时才关闭，避免旧异步任务关闭新打开的模态框。
                    if *s_inner.modal_kind.read() == ModalKind::DeleteConfirm {
                        s_inner.modal_kind.set(ModalKind::None);
                        s_inner.deleting_user.set(None);
                        s_inner.form_error.set(None);
                    }
                    s_inner.list_version.with_mut(|v| *v += 1);
                }
                Err(err) => {
                    if crate::api::handle_unauth(&err, auth_async, nav, bus_async).await {
                        return;
                    }
                    push_log_err(
                        bus_async,
                        HttpMethod::Delete,
                        &format!("/api/users/{target_id}"),
                        &err,
                    );
                }
            }
        });
    };

    rsx! {
        ConfirmDialog {
            open,
            title: confirm_delete_title,
            message,
            danger: true,
            loading: submitting,
            confirm_label: "确认删除 (DELETE)".to_string(),
            on_confirm,
            on_cancel,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_form_modal(
    t: &'static Translations,
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
    let title_str = if kind == ModalKind::Create {
        t.users_modal_create_title
    } else {
        t.users_modal_edit_title
    };
    let password_required = kind == ModalKind::Create;
    let submit_label = if kind == ModalKind::Create {
        t.users_modal_create_submit
    } else {
        t.users_modal_edit_submit
    };
    let password_placeholder_str = if kind == ModalKind::Create {
        t.users_form_password_placeholder_create
    } else {
        t.users_form_password_placeholder_edit
    };

    // Extract all t.* values to owned strings before closures to avoid lifetime issues
    let sys_editable = t.users_system_editable.to_string();
    let name_empty = t.users_name_empty.to_string();
    let email_empty = t.users_email_empty.to_string();
    let password_empty = t.users_password_empty.to_string();
    let form_name_label = t.users_form_name_label.to_string();
    let form_name_placeholder = t.users_form_name_placeholder.to_string();
    let form_email_label = t.users_form_email_label.to_string();
    let form_email_placeholder = t.users_form_email_placeholder.to_string();
    let form_password_label = t.users_form_password_label.to_string();
    let form_password_hint = t.users_form_password_hint.to_string();
    let role_label = t.users_form_role_label.to_string();
    let role_admin = t.users_form_role_admin.to_string();
    let role_user = t.users_form_role_user.to_string();
    let initial_role_label = t.users_form_initial_role_label.to_string();
    let balance_section_label = t.users_balance_section_label.to_string();
    let balance_current_label = t.users_balance_current_label.to_string();
    let balance_input_label = t.users_balance_input_label.to_string();
    let balance_input_placeholder = t.users_balance_input_placeholder.to_string();
    let balance_increase = t.users_balance_increase_btn.to_string();
    let balance_decrease = t.users_balance_decrease_btn.to_string();

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
                .set(Some(sys_editable.clone()));
            return;
        }
        // 同步校验：避免空字段浪费网络请求（Create 与 Edit 模式均需校验 name/email）
        if name.trim().is_empty() {
            signals_for_submit.form_error.set(Some(name_empty.clone()));
            return;
        }
        if email.trim().is_empty() {
            signals_for_submit.form_error.set(Some(email_empty.clone()));
            return;
        }
        if kind_now == ModalKind::Create && password.is_empty() {
            signals_for_submit
                .form_error
                .set(Some(password_empty.clone()));
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
                    if crate::api::handle_unauth(&err, auth_async, nav, bus_async).await {
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

    let on_increase =
        make_adjust_handler(signals, client.clone(), log_bus, auth.clone(), nav, 1, t);
    let on_decrease =
        make_adjust_handler(signals, client.clone(), log_bus, auth.clone(), nav, -1, t);

    rsx! {
        Modal {
            title: title_str.to_string(),
            on_close,
            open: true,
            disable_backdrop: submitting,
            div { class: "ws-form-stack",
                if let Some(err) = form_error.as_ref() {
                    p { class: "ws-form-error", "{err}" }
                }
                TextInput {
                    label: form_name_label.clone(),
                    placeholder: Some(form_name_placeholder.clone()),
                    value: signals.form_name,
                    required: true,
                    disabled: submitting,
                    name: Some("name".to_string()),
                }
                TextInput {
                    label: form_email_label.clone(),
                    placeholder: Some(form_email_placeholder.clone()),
                    value: signals.form_email,
                    input_type: InputType::Email,
                    required: true,
                    disabled: submitting,
                    name: Some("email".to_string()),
                }
                if kind == ModalKind::Create {
                    TextInput {
                        label: form_password_label.clone(),
                        placeholder: Some(password_placeholder_str.to_string()),
                        value: signals.form_password,
                        input_type: InputType::Password,
                        required: password_required,
                        disabled: submitting,
                        name: Some("password".to_string()),
                        hint: Some(
                            form_password_hint.clone(),
                        ),
                    }
                }
                if kind == ModalKind::Edit && current_role == "system" {
                    div { class: "ws-form-field",
                        label { class: "ws-form-label", "{role_label}" }
                        div { class: "ws-form-pill-group",
                            button {
                                r#type: "button",
                                class: if role_now == "admin" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_admin,
                                "{role_admin}"
                            }
                            button {
                                r#type: "button",
                                class: if role_now == "user" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_user,
                                "{role_user}"
                            }
                        }
                    }
                }
                // 创建模式：仅 system 角色可设置初始角色
                if kind == ModalKind::Create && current_role == "system" {
                    div { class: "ws-form-field",
                        label { class: "ws-form-label", "{initial_role_label}" }
                        div { class: "ws-form-pill-group",
                            button {
                                r#type: "button",
                                class: if role_now == "admin" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_admin,
                                "{role_admin}"
                            }
                            button {
                                r#type: "button",
                                class: if role_now == "user" { "ws-form-pill ws-form-pill--active" } else { "ws-form-pill" },
                                onclick: pick_user,
                                "{role_user}"
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
                            div { class: "ws-form-label", "{balance_section_label}" }
                            p { class: "ws-form-description",
                                "{balance_current_label}"
                                strong { "{format_balance(u.balance)}" }
                            }
                            TextInput {
                                label: balance_input_label.clone(),
                                placeholder: Some(balance_input_placeholder.clone()),
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
                                    "{balance_increase}"
                                }
                                button {
                                    class: "ws-btn ws-btn--primary ws-btn--pill",
                                    r#type: "button",
                                    disabled: submitting,
                                    onclick: on_decrease,
                                    "{balance_decrease}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn build_columns(t: &'static Translations) -> Vec<Column> {
    vec![
        Column::new(t.users_column_name).align(Align::Left),
        Column::new(t.users_column_email).align(Align::Left),
        Column::new(t.users_column_role).align(Align::Left),
        Column::new(t.users_column_balance)
            .width("w-24")
            .align(Align::Right),
        Column::new(t.users_column_created)
            .width("w-40")
            .align(Align::Left),
        Column::new(t.users_column_actions)
            .width("w-44")
            .align(Align::Center),
    ]
}

fn format_dt(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}
