//! 通用确认弹窗组件 —— 封装 Modal + 取消/确认按钮。
//!
//! 适用于需要用户二次确认的破坏性操作或重要操作，如登出所有设备、
//! 删除资源、重置数据等。样式文件：`assets/components/confirm_dialog.css`。
//!
//! # 用法
//! ```ignore
//! ConfirmDialog {
//!     open: *show_delete_confirm.read(),
//!     title: "确认删除".to_string(),
//!     message: "此操作不可撤销，确定要删除吗？".to_string(),
//!     danger: true,
//!     loading: *deleting.read(),
//!     on_confirm: move |_| { /* 执行删除 */ },
//!     on_cancel: move |_| show_delete_confirm.set(false),
//! }
//! ```

use dioxus::prelude::*;
use dioxus_icons::lucide::LoaderCircle;
use ui::Modal;

#[component]
pub fn ConfirmDialog(
    /// 是否显示弹窗
    open: bool,
    /// 弹窗标题
    title: String,
    /// 确认提示信息
    message: String,
    /// 点击确认时的回调
    on_confirm: EventHandler<MouseEvent>,
    /// 点击取消 / 关闭弹窗时的回调
    on_cancel: EventHandler<MouseEvent>,
    /// 确认操作是否为破坏性（使用危险样式按钮），默认 false
    #[props(default = false)]
    danger: bool,
    /// 确认按钮是否处于加载中状态，默认 false
    #[props(default = false)]
    loading: bool,
    /// 确认按钮文案，默认"确认"
    #[props(default = "确认".to_string())]
    confirm_label: String,
    /// 取消按钮文案，默认"取消"
    #[props(default = "取消".to_string())]
    cancel_label: String,
    /// 是否禁用点击遮罩层关闭弹窗，默认 true（避免误触）
    #[props(default = true)]
    disable_backdrop: bool,
) -> Element {
    let confirm_class = if danger {
        "ws-btn ws-btn--danger"
    } else {
        "ws-btn ws-btn--primary"
    };

    rsx! {
        // 始终加载 CSS，即使弹窗未打开（避免 FOUC）
        document::Link {
            rel: "stylesheet",
            href: asset!("/assets/components/confirm_dialog.css"),
        }

        if open {
            div { class: "ws-confirm-dialog",
                Modal {
                    title,
                    on_close: move |e| on_cancel.call(e),
                    disable_backdrop,

                    p { class: "ws-confirm-dialog__msg", "{message}" }

                    div { class: "ws-confirm-dialog__actions",
                        button {
                            class: "ws-btn ws-btn--secondary",
                            r#type: "button",
                            onclick: move |e| on_cancel.call(e),
                            "{cancel_label}"
                        }
                        button {
                            class: "{confirm_class}",
                            r#type: "button",
                            disabled: loading,
                            onclick: move |e| {
                                if !loading {
                                    on_confirm.call(e);
                                }
                            },
                            if loading {
                                LoaderCircle { class: "ws-btn__spinner" }
                            }
                            "{confirm_label}"
                        }
                    }
                }
            }
        }
    }
}
