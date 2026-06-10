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
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GET" => Self::Get,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "DELETE" => Self::Delete,
            _ => Self::Get,
        }
    }
}

#[allow(dead_code)]
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

pub fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
