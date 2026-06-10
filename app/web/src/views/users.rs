//! Users 管理视图（admin）。
//!
//! Phase 3 —— 完整接入 `client-api` 的 list / create / update / delete。
//! 通过 `LogBus` 写入 toast + console。

use chrono::{DateTime, Utc};
use client_api::{ClientError, UserResponse};
use dioxus::prelude::dioxus_router::Navigator;
use dioxus::prelude::*;

use ui::{
    Align, Badge, BadgeVariant, Button, ButtonType, Column, DataTable, InputType, Modal, TextInput,
};

use crate::auth::AuthState;
use crate::components::{HttpMethod, LogBus, LogKind};

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

    let signals = UsersSignals {
        modal_kind: use_signal(|| ModalKind::None),
        editing_user: use_signal(|| Option::<UserResponse>::None),
        deleting_user: use_signal(|| Option::<UserResponse>::None),
        form_name: use_signal(String::new),
        form_email: use_signal(String::new),
        form_password: use_signal(String::new),
        form_role: use_signal(|| "user".to_string()),
        submitting: use_signal(|| false),
        form_error: use_signal(|| Option::<String>::None),
        list_version,
    };

    {
        let client = auth.client.clone();
        let bus = log_bus;
        let auth_for_effect = auth.clone();
        use_effect(move || {
            let _ = list_version.cloned();
            let client = client.clone();
            let mut list = list;
            let bus = bus;
            let auth_inner = auth_for_effect.clone();
            spawn(async move {
                list.set(ListState::Loading);
                let res = client.list_users(1, 20).await;
                match res {
                    Ok(page) => {
                        push_log_ok(bus, HttpMethod::Get, "/api/users");
                        list.set(ListState::Loaded(page.items));
                    }
                    Err(err) => {
                        push_log_err(bus, HttpMethod::Get, "/api/users", &err);
                        if crate::api::handle_unauth(&err, auth_inner, nav) {
                            return;
                        }
                        list.set(ListState::Error(humanize_error(&err)));
                    }
                }
            });
        });
    }

    let list_snapshot = list.cloned();
    let kind_snapshot = *signals.modal_kind.read();
    let form_error_snapshot = signals.form_error.read().clone();
    let submitting_snapshot = *signals.submitting.read();
    let editing_snapshot = signals.editing_user.read().clone();
    let deleting_snapshot = signals.deleting_user.read().clone();

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
                        i { class: "fa-solid fa-shield-halved" }
                        "require_admin 中间件保护区域"
                    }
                    Button { onclick: open_create,
                        i { class: "fa-solid fa-plus" }
                        "创建新用户 (POST)"
                    }
                }
            }

            {render_table(list_snapshot, signals)}

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
                )
            }
        }
    }
}

fn render_table(list_snapshot: ListState, signals: UsersSignals) -> Element {
    match list_snapshot {
        ListState::Loading => rsx! {
            div { class: "ws-users__status",
                i { class: "fa-solid fa-spinner fa-spin" }
                "正在加载用户列表…"
            }
        },
        ListState::Error(msg) => rsx! {
            div { class: "ws-users__status ws-users__status--error",
                i { class: "fa-solid fa-triangle-exclamation" }
                "{msg}"
            }
        },
        ListState::Loaded(items) => {
            let columns = build_columns();
            let rows: Vec<Element> = items.into_iter().map(|u| row_element(u, signals)).collect();
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

fn row_element(u: UserResponse, signals: UsersSignals) -> Element {
    let u_for_edit = u.clone();
    let u_for_delete = u;
    let mut s_edit = signals;
    let mut s_delete = signals;
    let edit_handler = move |_: MouseEvent| {
        s_edit.form_name.set(u_for_edit.name.clone());
        s_edit.form_email.set(u_for_edit.email.clone());
        s_edit.form_password.set(String::new());
        s_edit.form_role.set(u_for_edit.role.clone());
        s_edit.form_error.set(None);
        s_edit.editing_user.set(Some(u_for_edit.clone()));
        s_edit.modal_kind.set(ModalKind::Edit);
    };
    let id_for_key = u_for_delete.id;
    let name = u_for_delete.name.clone();
    let email = u_for_delete.email.clone();
    let role = u_for_delete.role.clone();
    let created = u_for_delete.created_at;
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
                } else {
                    Badge { variant: BadgeVariant::User, "普通用户" }
                }
            }
            td { class: "ws-table__mono", "{format_dt(&created)}" }
            td {
                div { class: "ws-table__row-actions",
                    button {
                        class: "ws-table__action",
                        title: "编辑",
                        onclick: edit_handler,
                        i { class: "fa-solid fa-pen" }
                    }
                    button {
                        class: "ws-table__action ws-table__action--danger",
                        title: "删除",
                        onclick: delete_handler,
                        i { class: "fa-solid fa-trash" }
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
    signals.submitting.set(false);
    signals.form_error.set(None);
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
) -> Element {
    if kind == ModalKind::None {
        return VNode::empty();
    }

    if kind == ModalKind::DeleteConfirm {
        return render_delete_modal(
            form_error, submitting, deleting, signals, client, log_bus, auth, nav,
        );
    }

    render_form_modal(
        kind, form_error, submitting, editing, signals, client, log_bus, auth, nav,
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
) -> Element {
    let on_close = move |_: MouseEvent| close_all(signals);
    let on_cancel = move |_: MouseEvent| close_all(signals);
    let mut signals_for_submit = signals;
    let on_confirm = move |_: MouseEvent| {
        let Some(u) = signals_for_submit.deleting_user.cloned() else {
            signals_for_submit
                .form_error
                .set(Some("未选择目标用户".into()));
            return;
        };
        let target_id = u.id;
        signals_for_submit.submitting.set(true);
        signals_for_submit.form_error.set(None);
        let client_async = client.clone();
        let bus_async = log_bus;
        let mut s_async = signals_for_submit;
        let auth_async = auth.clone();
        spawn(async move {
            let res = client_async.delete_user(target_id).await;
            if res.is_ok() {
                push_log_ok(
                    bus_async,
                    HttpMethod::Delete,
                    &format!("/api/users/{target_id}"),
                );
            } else if let Err(ref err) = res {
                push_log_err(
                    bus_async,
                    HttpMethod::Delete,
                    &format!("/api/users/{target_id}"),
                    err,
                );
            }
            s_async.submitting.set(false);
            match res {
                Ok(_) => {
                    s_async.modal_kind.set(ModalKind::None);
                    s_async.deleting_user.set(None);
                    s_async.form_error.set(None);
                    s_async.list_version.with_mut(|v| *v += 1);
                }
                Err(err) => {
                    if crate::api::handle_unauth(&err, auth_async, nav) {
                        return;
                    }
                    s_async.form_error.set(Some(humanize_error(&err)));
                }
            }
        });
    };
    rsx! {
        Modal { title: "确认删除", on_close, open: true,
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
    let on_submit = move |_: MouseEvent| {
        let name = signals_for_submit.form_name.cloned();
        let email = signals_for_submit.form_email.cloned();
        let password = signals_for_submit.form_password.cloned();
        let role = signals_for_submit.form_role.cloned();
        let editing_id = editing_for_submit.as_ref().map(|u| u.id);
        let kind_now = kind;
        let client_async = client.clone();
        let bus_async = log_bus;
        let mut s_async = signals_for_submit;
        let auth_async = auth.clone();
        signals_for_submit.submitting.set(true);
        signals_for_submit.form_error.set(None);
        spawn(async move {
            let res = match kind_now {
                ModalKind::Create => {
                    if password.is_empty() {
                        s_async.form_error.set(Some("密码不能为空".into()));
                        s_async.submitting.set(false);
                        return;
                    }
                    let r = client_async.create_user(&email, &password, &name).await;
                    if r.is_ok() {
                        push_log_ok(bus_async, HttpMethod::Post, "/api/users");
                    } else if let Err(ref err) = r {
                        push_log_err(bus_async, HttpMethod::Post, "/api/users", err);
                    }
                    r.map(|_| ())
                }
                ModalKind::Edit => {
                    let Some(id) = editing_id else {
                        s_async.form_error.set(Some("缺少用户 ID".into()));
                        s_async.submitting.set(false);
                        return;
                    };
                    let r = client_async
                        .update_user(
                            id,
                            Some(email.clone()),
                            Some(name.clone()),
                            Some(role.clone()),
                        )
                        .await;
                    if r.is_ok() {
                        push_log_ok(bus_async, HttpMethod::Put, &format!("/api/users/{id}"));
                    } else if let Err(ref err) = r {
                        push_log_err(bus_async, HttpMethod::Put, &format!("/api/users/{id}"), err);
                    }
                    r.map(|_| ())
                }
                _ => unreachable!(),
            };
            s_async.submitting.set(false);
            match res {
                Ok(_) => {
                    s_async.modal_kind.set(ModalKind::None);
                    s_async.editing_user.set(None);
                    s_async.form_name.set(String::new());
                    s_async.form_email.set(String::new());
                    s_async.form_password.set(String::new());
                    s_async.form_role.set("user".to_string());
                    s_async.form_error.set(None);
                    s_async.list_version.with_mut(|v| *v += 1);
                }
                Err(err) => {
                    if crate::api::handle_unauth(&err, auth_async, nav) {
                        return;
                    }
                    s_async.form_error.set(Some(humanize_error(&err)));
                }
            }
        });
    };

    let role_now = signals.form_role.cloned();

    rsx! {
        Modal { title: title.to_string(), on_close, open: true,
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
                if kind == ModalKind::Edit {
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
                Button {
                    button_type: ButtonType::Submit,
                    full_width: true,
                    disabled: submitting,
                    loading: submitting,
                    onclick: on_submit,
                    "{submit_label}"
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
        Column::new("实例孵化时间").width("w-40").align(Align::Left),
        Column::new("操作")
            .width("w-32")
            .align(Align::Center),
    ]
}

fn format_dt(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

fn humanize_error(err: &ClientError) -> String {
    match err {
        ClientError::Network(msg) => format!("网络异常: {msg}"),
        ClientError::ServerError(status, body) => format!("服务器错误 (HTTP {status}): {body}"),
        ClientError::Other(status, body) => {
            let code = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| v.get("error").and_then(|c| c.as_str().map(String::from)))
                .unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str().map(String::from)))
                .unwrap_or_else(|| body.clone());
            match (status, code.as_str()) {
                (401, _) => "未登录或会话已过期".to_string(),
                (403, _) => "权限不足 (需 admin)".to_string(),
                (404, _) => "用户不存在".to_string(),
                (_, "validation_error") => format!("参数错误: {msg}"),
                (_, "conflict") => "操作冲突（邮箱已存在或违反约束）".to_string(),
                _ => format!("请求失败 (HTTP {status}): {msg}"),
            }
        }
        ClientError::RateLimited(_) => "请求过于频繁，请稍后再试".to_string(),
        ClientError::Deserialization(msg) => format!("响应解析失败: {msg}"),
        ClientError::Config(msg) => format!("客户端配置错误: {msg}"),
        _ => format!("未知错误: {err}"),
    }
}

fn push_log_ok(mut bus: LogBus, method: HttpMethod, path: &str) {
    bus.push(
        method,
        path.to_string(),
        "200 OK".to_string(),
        LogKind::Success,
    );
}

fn push_log_err(mut bus: LogBus, method: HttpMethod, path: &str, err: &ClientError) {
    let status = match err {
        ClientError::Other(s, _) | ClientError::ServerError(s, _) => s.to_string(),
        _ => "ERR".to_string(),
    };
    bus.push(method, path.to_string(), status, LogKind::Error);
}
