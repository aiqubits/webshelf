use dioxus::prelude::*;

/// 通用数据表 —— 按 DESIGN.md §3.9 规格实现。
///
/// 风格：毛玻璃容器、24px 圆角、表头 `bg-slate-50/70` + 大写 caption 字体、
/// 行 hover `bg-slate-50/40`。
///
/// 本组件**不**关心数据渲染逻辑 —— 调用方将每行预渲染为 `<tr>` 后传入，
/// 这样可以避开 Dioxus 中 `fn(&T) -> Element` 闭包作为 prop 的限制，
/// 同时让 `T` 可以是任何类型（包括 `client_api::UserResponse`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    fn class(&self) -> &'static str {
        match self {
            Self::Left => "ws-table__align--left",
            Self::Center => "ws-table__align--center",
            Self::Right => "ws-table__align--right",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub header: String,
    /// Tailwind 风格的宽度类，如 `"w-16"`、`"w-32"`、`"w-40"`。
    pub width: Option<String>,
    pub align: Align,
}

impl Column {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            width: None,
            align: Align::Left,
        }
    }

    pub fn width(mut self, w: impl Into<String>) -> Self {
        self.width = Some(w.into());
        self
    }

    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
}

#[component]
pub fn DataTable(
    columns: Vec<Column>,
    rows: Vec<Element>,
    #[props(default)] empty: Option<Element>,
) -> Element {
    rsx! {
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/styling/data_table.css"),
        }
        div { class: "ws-table",
            table { class: "ws-table__table",
                thead { class: "ws-table__head",
                    tr {
                        for col in columns.iter() {
                            th {
                                class: "ws-table__th {col.align.class()} {col.width.clone().unwrap_or_default()}",
                                scope: "col",
                                "{col.header}"
                            }
                        }
                    }
                }
                tbody { class: "ws-table__body",
                    if rows.is_empty() {
                        if let Some(placeholder) = empty {
                            tr {
                                td {
                                    class: "ws-table__empty",
                                    colspan: columns.len(),
                                    {placeholder}
                                }
                            }
                        } else {
                            tr {
                                td {
                                    class: "ws-table__empty",
                                    colspan: columns.len(),
                                    "暂无数据"
                                }
                            }
                        }
                    } else {
                        for row in rows.iter() {
                            {row}
                        }
                    }
                }
            }
        }
    }
}
