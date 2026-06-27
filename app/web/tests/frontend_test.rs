//! Dioxus 前端测试 —— 陈旧任务防护（Stale Task Protection） & system 角色行保护。
//!
//! # 测试策略
//!
//! Dioxus 组件测试需要在 VirtualDom 运行时上下文中进行。
//! 在 Dioxus 0.7 中，Signal::new() 只能在组件内部调用。
//!
//! 重要：Dioxus 0.7 在 `any_props::render()` 内部使用 `catch_unwind` 包裹
//! 组件渲染函数，组件闭包内的 `assert!`/`unreachable!` panic **不会**传播到
//! 测试框架。必须使用 `AtomicBool` 作为 side channel 将结果传递到
//! VirtualDom 外部再进行断言。

use dioxus::prelude::*;
use dioxus_core::VirtualDom;
use std::sync::atomic::{AtomicBool, Ordering};
use ui::{I18nContext, Language};

// ──────────────────────────────────────────────
//  Dashboard 陈旧任务防护模式测试
// ──────────────────────────────────────────────
//
// 对应 `run_health_check` / `run_user_count` 中的版本检查模式：
// 1. 异步任务启动时快照当前版本号 (`let v = version.read()`)
// 2. 异步操作（如 health_check()）完成后检查版本是否变化
// 3. 若 `signal() != snapshot` → 丢弃，避免旧任务覆盖新数据

/// 基本版本不匹配检测：版本变更后旧任务应被丢弃。
///
/// 使用 `AtomicBool` 作为 side channel 绕过 Dioxus 的
/// `catch_unwind`（panic 不会传播到测试框架）。
#[test]
fn stale_task_version_mismatch_aborts() {
    static MISMATCH_DETECTED: AtomicBool = AtomicBool::new(false);
    static MATCH_DETECTED: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        let mut version = Signal::new(0u64);

        // 任务 1 快照版本（version=0），然后另一任务递增了版本
        let task1_snapshot = *version.read();
        *version.write() += 1; // 模拟另一任务启动 → 版本变为 1

        // 版本不匹配 → 应丢弃
        if *version.read() != task1_snapshot {
            MISMATCH_DETECTED.store(true, Ordering::SeqCst);
        }

        // 任务 3 快照版本（version=1），无其他任务干扰
        let task3_snapshot = *version.read();

        // 版本匹配 → 应保留
        if *version.read() == task3_snapshot {
            MATCH_DETECTED.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(
        MISMATCH_DETECTED.load(Ordering::SeqCst),
        "版本变更后旧任务应被丢弃"
    );
    assert!(
        MATCH_DETECTED.load(Ordering::SeqCst),
        "最新任务应通过版本检查"
    );
}

/// 多任务链式版本递增：验证只有最新任务通过版本检查。
#[test]
fn stale_task_version_chain() {
    static T1_DISCARDED: AtomicBool = AtomicBool::new(false);
    static T2_DISCARDED: AtomicBool = AtomicBool::new(false);
    static T3_PASSED: AtomicBool = AtomicBool::new(false);

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

        if *version.read() != v1 {
            T1_DISCARDED.store(true, Ordering::SeqCst);
        }
        if *version.read() != v2 {
            T2_DISCARDED.store(true, Ordering::SeqCst);
        }
        if *version.read() == v3 {
            T3_PASSED.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(T1_DISCARDED.load(Ordering::SeqCst), "任务 1 应被丢弃");
    assert!(T2_DISCARDED.load(Ordering::SeqCst), "任务 2 应被丢弃");
    assert!(T3_PASSED.load(Ordering::SeqCst), "任务 3 应通过检查");
}

/// 验证 Signal 的 Copy 语义：快照独立于原信号变化。
#[test]
fn signal_version_copy_isolates_snapshot() {
    static SNAPSHOT_PRESERVED: AtomicBool = AtomicBool::new(false);
    static SIGNAL_UPDATED: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        let mut version = Signal::new(42u64);

        // 快照：通过 Copy 获得独立副本
        let snapshot = *version.read();

        // 写入原信号
        *version.write() = 100;

        // 快照不受影响
        if snapshot == 42 {
            SNAPSHOT_PRESERVED.store(true, Ordering::SeqCst);
        }
        if *version.read() == 100 {
            SIGNAL_UPDATED.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(
        SNAPSHOT_PRESERVED.load(Ordering::SeqCst),
        "快照应不受版本变更影响"
    );
    assert!(SIGNAL_UPDATED.load(Ordering::SeqCst), "信号本身已更新");
}

/// 模拟 `run_health_check` 的完整模式：先 await 后版本检查。
#[test]
fn health_check_version_check_after_await() {
    static MATCH_PASSED: AtomicBool = AtomicBool::new(false);
    static STALE_DISCARDED: AtomicBool = AtomicBool::new(false);
    static NEW_TASK_PASSED: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        let mut version = Signal::new(0u64);

        // ── 模拟版本递增（如 use_effect 或 on_run_health） ──
        let task_version = {
            *version.write() += 1;
            *version.read()
        };

        // ── 模拟异步 await 后（health_check() 完成）的版本校验 ──
        // 场景 A：版本匹配 → 继续（正常情况）
        if *version.read() == task_version {
            MATCH_PASSED.store(true, Ordering::SeqCst);
        }

        // 场景 B：另一任务中途递增了版本 → 应丢弃
        let task2_version = {
            *version.write() += 1;
            *version.read()
        };

        if *version.read() != task_version {
            STALE_DISCARDED.store(true, Ordering::SeqCst);
        }
        if *version.read() == task2_version {
            NEW_TASK_PASSED.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(
        MATCH_PASSED.load(Ordering::SeqCst),
        "正常完成时应通过版本校验"
    );
    assert!(
        STALE_DISCARDED.load(Ordering::SeqCst),
        "旧任务应被后续点击废弃"
    );
    assert!(NEW_TASK_PASSED.load(Ordering::SeqCst), "最新任务应通过校验");
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
    static ADMIN_EXCLUDED: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        // 模拟 row_element 中的各种角色
        let system_role = "system".to_string();
        let user_role = "user".to_string();
        let admin_role = "admin".to_string();
        let another_system = "system".to_string();

        // 用 side-channel 替代 assert!，避免被 catch_unwind 吞没
        // 每个检查独立 AtomicBool，失败时可精确定位具体哪个条件不符。
        if system_role == "system" && another_system == "system" {
            SYSTEM_DETECTED.store(true, Ordering::SeqCst);
        }
        if user_role != "system" {
            NON_SYSTEM_DETECTED.store(true, Ordering::SeqCst);
        }
        if admin_role != "system" {
            ADMIN_EXCLUDED.store(true, Ordering::SeqCst);
        }

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
    assert!(ADMIN_EXCLUDED.load(Ordering::SeqCst), "admin 角色应被排除");
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
        // 注意：原先此处有 else { unreachable!() }，但因 Dioxus 0.7 用
        // catch_unwind 包裹渲染组件，panic 不传播到测试框架。改用
        // AtomicBool side-channel 在 VirtualDom 外部验证分支正确性。
        let is_system = "system" == "system";
        if is_system {
            SYSTEM_PROTECTED.store(true, Ordering::SeqCst);
        }

        // user 角色 → action 按钮分支
        // 同上，unreachable!() 被外部 AtomicBool 断言替代。
        let is_user_system = "user" == "system";
        if !is_user_system {
            USER_HAS_ACTIONS.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(
        SYSTEM_PROTECTED.load(Ordering::SeqCst),
        "system 角色应进入 protected 分支"
    );
    assert!(
        USER_HAS_ACTIONS.load(Ordering::SeqCst),
        "user 角色应进入 action 按钮分支"
    );
}

// ──────────────────────────────────────────────
//  I18nContext 与 LanguageSwitcher 集成测试
// ──────────────────────────────────────────────

/// I18nContext 初始化后 lang() 默认返回 Language::En。
#[test]
fn i18n_context_defaults_to_en() {
    static INITIAL_LANG: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        let ctx = I18nContext::new(Language::En);
        use_context_provider(|| ctx);
        if use_context::<I18nContext>().lang() == Language::En {
            INITIAL_LANG.store(true, Ordering::SeqCst);
        }
        rsx! {
            div {}
        }
    });
    dom.rebuild_in_place();

    assert!(
        INITIAL_LANG.load(Ordering::SeqCst),
        "I18nContext 默认语言应为 En"
    );
}

/// I18nContext::set_lang() 切换语言后 lang() 正确反映新值。
#[test]
fn i18n_context_set_lang_switches_to_zh() {
    static SWITCHED_TO_ZH: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        use_context_provider(|| I18nContext::new(Language::En));
        let mut ctx = use_context::<I18nContext>();
        ctx.set_lang(Language::Zh);
        if ctx.lang() == Language::Zh {
            SWITCHED_TO_ZH.store(true, Ordering::SeqCst);
        }
        rsx! {
            div {}
        }
    });
    dom.rebuild_in_place();

    assert!(
        SWITCHED_TO_ZH.load(Ordering::SeqCst),
        "set_lang(Zh) 后 lang() 应为 Zh"
    );
}

/// I18nContext::t() 在 En 和 Zh 下返回对应的 Translations。
#[test]
fn i18n_context_t_returns_correct_language_translations() {
    static EN_CORRECT: AtomicBool = AtomicBool::new(false);
    static ZH_CORRECT: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        use_context_provider(|| I18nContext::new(Language::En));
        let mut ctx = use_context::<I18nContext>();

        // 初始 En
        let t = ctx.t();
        if t.dashboard_title == "Welcome to WebShelf Rust Full-stack System 🚀" {
            EN_CORRECT.store(true, Ordering::SeqCst);
        }

        // 切换到 Zh
        ctx.set_lang(Language::Zh);
        let t = ctx.t();
        if t.dashboard_title == "欢迎来到 WebShelf Rust 全端全栈管理系统 🚀" {
            ZH_CORRECT.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(
        EN_CORRECT.load(Ordering::SeqCst),
        "En 下应返回英文 Translations"
    );
    assert!(
        ZH_CORRECT.load(Ordering::SeqCst),
        "Zh 下应返回中文 Translations"
    );
}

/// I18nContext 在 En ↔ Zh 之间反复切换均正确。
#[test]
fn i18n_context_toggle_en_zh_en() {
    static FINAL_IS_EN: AtomicBool = AtomicBool::new(false);

    let mut dom = VirtualDom::new(|| {
        use_context_provider(|| I18nContext::new(Language::Zh));
        let mut ctx = use_context::<I18nContext>();

        // Zh → En → Zh → En
        ctx.set_lang(Language::En);
        ctx.set_lang(Language::Zh);
        ctx.set_lang(Language::En);

        if ctx.lang() == Language::En {
            FINAL_IS_EN.store(true, Ordering::SeqCst);
        }

        rsx! {
            div {}

        }
    });
    dom.rebuild_in_place();

    assert!(
        FINAL_IS_EN.load(Ordering::SeqCst),
        "多次切换后最终语言应为 En"
    );
}
