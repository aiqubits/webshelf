//! LogBus —— 全局 API 调用日志总线。
//!
//! 同时为 `Toast` 通知与 Dashboard 的 `CodeConsole` 提供数据源。
//! 写入与读取都通过 `Signal<Vec<LogEntry>>` 完成，自动驱动所有订阅者重渲染。

use dioxus::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "DELETE" => Some(Self::Delete),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Success,
    Error,
    /// 重要操作（如删除）—— 显示为玫瑰色但与 Error 视觉上区分
    Important,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub id: Uuid,
    pub method: HttpMethod,
    pub path: String,
    pub status: String,
    pub kind: LogKind,
    pub created_at_ms: u64,
}

impl LogEntry {
    pub fn new(
        method: HttpMethod,
        path: impl Into<String>,
        status: impl Into<String>,
        kind: LogKind,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            method,
            path: path.into(),
            status: status.into(),
            kind,
            created_at_ms: now_unix_ms(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct LogBus {
    pub entries: Signal<Vec<LogEntry>>,
}

impl LogBus {
    /// 在 App 组件创建一次。
    pub fn new() -> Self {
        Self {
            entries: Signal::new(Vec::new()),
        }
    }

    /// 推入一条新日志。
    pub fn push(
        &mut self,
        method: HttpMethod,
        path: impl Into<String>,
        status: impl Into<String>,
        kind: LogKind,
    ) {
        let entry = LogEntry::new(method, path, status, kind);
        let mut entries = self.entries.write();
        const MAX_ENTRIES: usize = 200;
        if entries.len() >= MAX_ENTRIES {
            let drop = entries.len() + 1 - MAX_ENTRIES;
            entries.drain(0..drop);
        }
        entries.push(entry);
    }

    /// 移除指定 id 的日志。
    #[allow(dead_code)]
    pub fn remove(&mut self, id: Uuid) {
        self.entries.write().retain(|e| e.id != id);
    }

    /// 清空。
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.entries.write().clear();
    }
}

/// 向 LogBus 写入一条结果日志——成功记为 Success，失败记为 Error。
///
/// 这是 auth.rs / dashboard.rs / users.rs 中原有多份 push_log 实现的统一入口。
pub fn push_log_result<T>(
    mut bus: LogBus,
    method: HttpMethod,
    path: &str,
    res: &Result<T, client_api::ClientError>,
) {
    match res {
        Ok(_) => bus.push(method, path, "200 OK", LogKind::Success),
        Err(err) => {
            let status = err.status_or_label();
            bus.push(method, path, status, LogKind::Error);
        }
    }
}

/// 向 LogBus 写入一条成功日志。
pub fn push_log_ok(mut bus: LogBus, method: HttpMethod, path: &str) {
    bus.push(method, path, "200 OK", LogKind::Success);
}

/// 向 LogBus 写入一条错误日志（从 ClientError 提取状态码）。
pub fn push_log_err(
    mut bus: LogBus,
    method: HttpMethod,
    path: &str,
    err: &client_api::ClientError,
) {
    let status = err.status_or_label();
    bus.push(method, path, status, LogKind::Error);
}

pub fn now_unix_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

pub fn now_unix_secs() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}
