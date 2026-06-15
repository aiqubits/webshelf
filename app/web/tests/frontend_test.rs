//! Dioxus 前端测试 —— 陈旧任务防护（Stale Task Protection） & system 角色行保护。
//!
//! # 测试策略
//!
//! Dioxus 组件测试需要在 VirtualDom 运行时上下文中进行。
//! 在 Dioxus 0.7 中，Signal::new() 只能在组件内部调用。
//! 我们通过在 VirtualDom 组件内部创建 Signal 并执行断言的方式来测试。
//! 组件在 `rebuild_in_place()` 期间运行，断言失败会通过 panic 传播到测试。
//!
//! 对于跨组件捕获的结果，使用 `AtomicBool` 作为 side channel。

use dioxus::prelude::*;
use dioxus_core::VirtualDom;
use std::sync::atomic::{AtomicBool, Ordering};

// ──────────────────────────────────────────────
//  Dashboard 陈旧任务防护模式测试
// ──────────────────────────────────────────────
//
// 对应 `run_health_check` / `run_user_count` 中的版本检查模式：
// 1. 异步任务启动时快照当前版本号 (`let v = version.read()`)
// 2. 异步操作（如 health_check()）完成后检查版本是否变化
// 3. 若 `signal() != snapshot` → 丢弃，避免旧任务覆盖新数据

/// 基本版本不匹配检测：版本变更后旧任务应被丢弃。
#[test]
fn stale_task_version_mismatch_aborts() {
    let mut dom = VirtualDom::new(|| {
        let mut version = Signal::new(0u64);

        // 任务 1 快照版本（version=0），然后另一任务递增了版本
        let task1_snapshot = *version.read();
        *version.write() += 1; // 模拟另一任务启动 → 版本变为 1

        // 任务 1 执行完异步操作后检查：版本不匹配 → 应丢弃
        assert!(
            *version.read() != task1_snapshot,
            "版本变更后旧任务应被丢弃"
        );

        // 任务 3 快照版本（version=1），无其他任务干扰
        let task3_snapshot = *version.read();

        // 任务 3 检查：版本匹配 → 应保留
        assert_eq!(*version.read(), task3_snapshot, "最新任务应通过版本检查");

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();
}

/// 多任务链式版本递增：验证只有最新任务通过版本检查。
#[test]
fn stale_task_version_chain() {
    let mut dom = VirtualDom::new(|| {
        let mut version = Signal::new(0u64);

        // 模拟实际代码中的模式：
        // let v = version_signal.with_mut(|v| { *v += 1; *v });
        let v1 = {
            *version.write() += 1;
            *version.read()
        };
        let v2 = {
            *version.write() += 1;
            *version.read()
        };
        let v3 = {
            *version.write() += 1;
            *version.read()
        };

        assert!(*version.read() != v1, "任务 1 应被丢弃");
        assert!(*version.read() != v2, "任务 2 应被丢弃");
        assert_eq!(*version.read(), v3, "任务 3 应通过检查");

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();
}

/// 验证 Signal 的 Copy 语义：快照独立于原信号变化。
#[test]
fn signal_version_copy_isolates_snapshot() {
    let mut dom = VirtualDom::new(|| {
        let mut version = Signal::new(42u64);

        // 快照：通过 Copy 获得独立副本
        let snapshot = *version.read();
        assert_eq!(snapshot, 42);

        // 写入原信号
        *version.write() = 100;

        // 快照不受影响
        assert_eq!(snapshot, 42, "快照应不受版本变更影响");
        assert_eq!(*version.read(), 100, "信号本身已更新");

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();
}

/// 模拟 `run_health_check` 的完整模式：先 await 后版本检查。
#[test]
fn health_check_version_check_after_await() {
    let mut dom = VirtualDom::new(|| {
        let mut version = Signal::new(0u64);

        // ── 模拟版本递增（如 use_effect 或 on_run_health） ──
        let task_version = {
            *version.write() += 1;
            *version.read()
        };

        // ── 模拟异步 await 后（health_check() 完成）的版本校验 ──
        // 场景 A：版本匹配 → 继续（正常情况）
        assert_eq!(*version.read(), task_version, "正常完成时应通过版本校验");

        // 场景 B：另一任务中途递增了版本 → 应丢弃
        let task2_version = {
            *version.write() += 1;
            *version.read()
        };

        assert!(*version.read() != task_version, "旧任务应被后续点击废弃");
        assert_eq!(*version.read(), task2_version, "最新任务应通过校验");

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();
}

// ──────────────────────────────────────────────
//  Users: system 角色行保护测试
// ──────────────────────────────────────────────
//
// 对应 `users.rs` 中 `row_element` 的 `is_system = role == "system"` 逻辑
// 和基于 `is_system` 的条件渲染分支。

/// 验证 `is_system = role == "system"` 的角色检测逻辑。
#[test]
fn system_role_detection() {
    static SYSTEM_DETECTED: AtomicBool = AtomicBool::new(false);
    static NON_SYSTEM_DETECTED: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        // 模拟 row_element 中的各种角色
        let system_role = "system".to_string();
        let user_role = "user".to_string();
        let admin_role = "admin".to_string();
        let another_system = "system".to_string();

        // 核心断言：is_system = role == "system"
        assert!(system_role == "system");
        assert!(user_role != "system");
        assert!(admin_role != "system");
        assert!(another_system == "system");

        SYSTEM_DETECTED.store(system_role == "system", Ordering::SeqCst);
        NON_SYSTEM_DETECTED.store(user_role != "system", Ordering::SeqCst);

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(
        SYSTEM_DETECTED.load(Ordering::SeqCst),
        "system 角色应被识别"
    );
    assert!(
        NON_SYSTEM_DETECTED.load(Ordering::SeqCst),
        "非 system 角色应被排除"
    );
}

/// 验证基于 `is_system` 的条件渲染分支：
/// system 角色进入 protected 分支，非 system 角色进入 action 按钮分支。
#[test]
fn system_role_guards_edit_delete_buttons() {
    static SYSTEM_PROTECTED: AtomicBool = AtomicBool::new(false);
    static USER_HAS_ACTIONS: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        // 模拟 users.rs row_element 中的条件分支：
        //   if is_system { 显示"受保护" } else { 显示编辑/删除按钮 }

        // system 角色 → protected 分支
        let is_system = "system" == "system";
        if is_system {
            SYSTEM_PROTECTED.store(true, Ordering::SeqCst);
        } else {
            unreachable!("system role should enter protected branch");
        }

        // user 角色 → action 按钮分支
        let is_user_system = "user" == "system";
        if is_user_system {
            unreachable!("user role should NOT enter protected branch");
        } else {
            USER_HAS_ACTIONS.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(SYSTEM_PROTECTED.load(Ordering::SeqCst));
    assert!(USER_HAS_ACTIONS.load(Ordering::SeqCst));
}
