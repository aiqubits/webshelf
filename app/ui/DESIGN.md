---
version: "2.0"
name: WebShelf Admin UI Design System
description: >
  从 prototype.html 提取的权威设计令牌与组件规格，作为 `app/ui` crate 中 Dioxus 组件实现的唯一设计提示词。
  设计语言以 Indigo-Glass 毛玻璃体系为核心：柔和多色渐变背景 + 毛玻璃半透明面板 + indigo→purple 渐变品牌按钮，
  辅以 Plus Jakarta Sans 字体族的轻盈现代感，以及克制的 hover 微动效与彩色装饰光晕。

colors:
  brand-indigo: "#6366f1"
  brand-indigo-deep: "#4f46e5"
  brand-indigo-dark: "#4338ca"
  brand-indigo-light: "#818cf8"
  brand-purple: "#a855f7"
  brand-gradient: "linear-gradient(135deg, #6366f1 0%, #a855f7 100%)"
  page-gradient: "linear-gradient(135deg, #f0f4ff 0%, #fbf8ff 40%, #fff4f4 100%)"
  page-gradient-start: "#f0f4ff"
  page-gradient-mid: "#fbf8ff"
  page-gradient-end: "#fff4f4"
  glass-bg: "rgba(255, 255, 255, 0.7)"
  glass-bg-heavy: "rgba(255, 255, 255, 0.85)"
  glass-bg-toast: "rgba(255, 255, 255, 0.9)"
  text-primary: "#1e293b"
  text-secondary: "#475569"
  text-tertiary: "#64748b"
  text-muted: "#94a3b8"
  text-muted-soft: "#cbd5e1"
  border-soft: "rgba(255, 255, 255, 0.5)"
  border-glass: "rgba(226, 232, 240, 0.8)"
  border-default: "#e2e8f0"
  border-light: "#f1f5f9"
  semantic-success-bg: "#ecfdf5"
  semantic-success-text: "#059669"
  semantic-success-border: "#a7f3d0"
  semantic-warning-bg: "#fffbeb"
  semantic-warning-text: "#d97706"
  semantic-warning-border: "#fde68a"
  semantic-error-text: "#e11d48"
  semantic-error-border: "#fecdd3"
  accent-indigo-soft-bg: "#eef2ff"
  accent-indigo-soft-border: "#e0e7ff"
  accent-purple-soft-bg: "#faf5ff"
  accent-purple-soft-border: "#f3e8ff"
  accent-pink-soft-bg: "#fdf2f8"
  accent-pink-soft-border: "#fce7f3"
  code-bg: "#0f172a"
  code-text: "#cbd5e1"
  code-success: "#34d399"
  code-info: "#818cf8"
  code-amber: "#fbbf24"
  code-error: "#fb7185"
  overlay-bg: "rgba(15, 23, 42, 0.2)"
  input-bg: "rgba(255, 255, 255, 0.6)"

typography:
  font-family: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
  font-mono: "ui-monospace, SFMono-Regular, Menlo, monospace"
  heading-xl:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 20px
    fontWeight: 700
    lineHeight: 28px
  heading-lg:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 16px
    fontWeight: 700
    lineHeight: 24px
  heading-md:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 14px
    fontWeight: 700
    lineHeight: 20px
  body-default:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 14px
    fontWeight: 500
    lineHeight: 20px
  body-sm:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 12px
    fontWeight: 400
    lineHeight: 16px
  body-sm-medium:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 12px
    fontWeight: 500
    lineHeight: 16px
  caption:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 11px
    fontWeight: 700
    lineHeight: 16px
    letterSpacing: 0.05em
    textTransform: uppercase
  caption-mono:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: 12px
    fontWeight: 400
    lineHeight: 16px
  fine-print:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 10px
    fontWeight: 500
    lineHeight: 16px
  fine-print-tiny:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 9px
    fontWeight: 400
    lineHeight: 14px
  button-label:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 12px
    fontWeight: 600
    lineHeight: 16px
  nav-item:
    fontFamily: "Plus Jakarta Sans, Noto Sans SC, sans-serif"
    fontSize: 14px
    fontWeight: 500
    lineHeight: 20px

rounded:
  sm: 2px
  md: 4px
  base: 6px
  lg: 8px
  xl: 12px
  2xl: 16px
  3xl: 24px
  full: 9999px

spacing:
  xs: 4px
  sm: 6px
  md: 8px
  lg: 12px
  xl: 16px
  2xl: 20px
  3xl: 24px
  4xl: 32px

shadows:
  xs: "0 1px 2px 0 rgba(0, 0, 0, 0.03)"
  sm: "0 1px 2px 0 rgba(0, 0, 0, 0.05)"
  md: "0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -2px rgba(0, 0, 0, 0.1)"
  lg: "0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -4px rgba(0, 0, 0, 0.1)"
  xl: "0 20px 25px -5px rgba(0, 0, 0, 0.1), 0 8px 10px -6px rgba(0, 0, 0, 0.1)"
  brand-hover: "0 10px 20px -5px rgba(99, 102, 241, 0.4)"
  card-hover: "0 20px 25px -5px rgba(99, 102, 241, 0.08), 0 10px 10px -5px rgba(99, 102, 241, 0.03)"
  inner: "inset 0 2px 4px 0 rgba(0, 0, 0, 0.04)"

transitions:
  card: "all 0.3s cubic-bezier(0.16, 1, 0.3, 1)"
  button: "all 0.4s cubic-bezier(0.16, 1, 0.3, 1)"
  nav: "all 0.2s ease"
  default: "all 0.2s ease"

glass-effects:
  panel:
    background: "rgba(255, 255, 255, 0.7)"
    backdrop-filter: "blur(16px)"
    border: "1px solid {colors.border-soft}"
  sidebar:
    background: "rgba(255, 255, 255, 0.85)"
    backdrop-filter: "blur(20px)"
    border-right: "1px solid rgba(226, 232, 240, 0.8)"
  toast:
    background: "rgba(255, 255, 255, 0.9)"
    backdrop-filter: "blur(12px)"
    border: "1px solid rgba(241, 245, 249, 0.8)"

decorative-blurs:
  indigo-soft: "rgba(199, 210, 254, 0.2)"
  indigo-medium: "rgba(199, 210, 254, 0.3)"
  pink-soft: "rgba(251, 207, 232, 0.2)"
  purple-soft: "rgba(233, 213, 255, 0.3)"

components:
  app-shell:
    backgroundColor: "{colors.page-gradient}"
    minHeight: 100vh
    display: flex

  sidebar:
    width: 256px
    glassEffect: "{glass-effects.sidebar}"
    logoHeight: 80px
    logoBackground: "linear-gradient(to top right, #6366f1, #a855f7)"
    logoTextGradient: "linear-gradient(to right, #4f46e5, #9333ea)"
    activeItem:
      backgroundColor: "rgba(238, 242, 255, 0.6)"
      textColor: "{colors.brand-indigo-deep}"
    inactiveItem:
      textColor: "{colors.text-secondary}"
      hoverBackground: "rgba(248, 250, 252, 0.8)"
      hoverTextColor: "#0f172a"
    sectionLabel:
      typography: "{typography.caption}"
      textColor: "{colors.text-muted}"
    footer:
      backgroundColor: "rgba(248, 250, 252, 0.5)"

  top-header:
    height: 80px
    glassEffect: "{glass-effects.panel}"
    borderBottom: "1px solid rgba(241, 245, 249, 0.6)"
    sticky: true
    zIndex: 30

  search-input:
    backgroundColor: "rgba(241, 245, 249, 0.8)"
    borderColor: "rgba(226, 232, 240, 0.4)"
    typography: "{typography.body-sm}"
    rounded: "{rounded.xl}"
    focusBorderColor: "{colors.brand-indigo-light}"
    focusBackgroundColor: "#ffffff"

  online-badge:
    backgroundColor: "{colors.semantic-success-bg}"
    textColor: "{colors.semantic-success-text}"
    borderColor: "rgba(167, 243, 208, 0.5)"
    dotColor: "#10b981"
    fontSize: 11px
    fontWeight: 600
    rounded: "{rounded.full}"

  avatar:
    size: 40px
    rounded: "{rounded.full}"
    background: "linear-gradient(to top right, #f3e8ff, #e0e7ff)"
    border: "1px solid white"
    shadow: "{shadows.inner}"
    textColor: "{colors.brand-indigo-deep}"
    typography: "{typography.heading-md}"

  button-primary:
    background: "{colors.brand-gradient}"
    textColor: "#ffffff"
    typography: "{typography.button-label}"
    rounded: "{rounded.xl}"
    padding: "10px 16px"
    shadow: "{shadows.md}"
    hoverTransform: "translateY(-2px)"
    hoverShadow: "{shadows.brand-hover}"
    hoverFilter: "brightness(1.05)"
    transition: "{transitions.button}"

  hero-band:
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.3xl}"
    padding: "{spacing.3xl}"
    border: "1px solid rgba(199, 210, 254, 0.4)"
    shadow: "{shadows.sm}"
    shadowColor: "rgba(238, 242, 255, 1)"
    headingTypography: "{typography.heading-xl}"
    headingColor: "{colors.text-primary}"
    bodyTypography: "{typography.body-sm}"
    bodyColor: "{colors.text-tertiary}"

  card-stats:
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.2xl}"
    padding: "{spacing.2xl}"
    transition: "{transitions.card}"
    hoverTransform: "translateY(-4px)"
    hoverShadow: "{shadows.card-hover}"
    hoverBorderColor: "rgba(99, 102, 241, 0.2)"
    labelTypography: "{typography.body-sm-medium}"
    labelColor: "{colors.text-muted}"
    valueTypography: "{typography.heading-xl}"
    valueColor: "{colors.text-primary}"
    valueColorHealth: "{colors.semantic-success-text}"
    valueColorMiddleware: "{colors.semantic-warning-text}"
    iconContainer:
      size: 48px
      rounded: "{rounded.xl}"
      background: "corresponding semantic 50-shade solid color (e.g. #ecfdf5 for health, #eef2ff for users)"

  card-stats-icon-colors:
    health: "{colors.semantic-success-bg}"
    healthIcon: "#10b981"
    users: "{colors.accent-indigo-soft-bg}"
    usersIcon: "{colors.brand-indigo}"
    latency: "{colors.accent-purple-soft-bg}"
    latencyIcon: "{colors.brand-purple}"
    middleware: "{colors.semantic-warning-bg}"
    middlewareIcon: "#f59e0b"

  code-console:
    backgroundColor: "{colors.code-bg}"
    textColor: "{colors.code-text}"
    rounded: "{rounded.xl}"
    padding: "{spacing.xl}"
    shadow: "{shadows.inner}"
    typography: "{typography.caption-mono}"
    fontSize: 12px

  route-card:
    backgroundColor: "rgba(238, 242, 255, 0.5)"
    borderColor: "{colors.accent-indigo-soft-border}"
    rounded: "{rounded.xl}"
    padding: "10px"
    methodBadge:
      fontSize: 10px
      fontWeight: 700
      rounded: "{rounded.md}"
      width: 48px

  route-card-purple:
    backgroundColor: "rgba(250, 245, 255, 0.5)"
    borderColor: "#f3e8ff"

  route-card-pink:
    backgroundColor: "rgba(253, 242, 248, 0.5)"
    borderColor: "#fce7f3"

  data-table:
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.3xl}"
    shadow: "{shadows.sm}"
    borderColor: "{colors.border-light}"
    headerBackground: "rgba(248, 250, 252, 0.7)"
    headerTypography: "{typography.caption}"
    headerColor: "{colors.text-muted}"
    bodyTypography: "{typography.body-sm-medium}"
    bodyColor: "{colors.text-secondary}"
    headerCellPadding: "16px 24px"
    bodyCellPadding: "14px 24px"
    rowBorder: "1px solid rgba(241, 245, 249, 0.6)"
    rowHoverBackground: "rgba(248, 250, 252, 0.4)"

  role-badge-admin:
    backgroundColor: "{colors.accent-purple-soft-bg}"
    textColor: "#9333ea"
    borderColor: "#f3e8ff"
    rounded: "{rounded.base}"
  role-badge-user:
    backgroundColor: "rgba(241, 245, 249, 1)"
    textColor: "{colors.text-secondary}"
    borderColor: "rgba(226, 232, 240, 0.6)"
    rounded: "{rounded.base}"

  auth-form-card:
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.3xl}"
    padding: "{spacing.4xl}"
    shadow: "{shadows.xl}, 0 20px 25px -5px rgba(243, 232, 255, 0.3)"
    maxWidth: 448px

  auth-tab-container:
    backgroundColor: "rgba(241, 245, 249, 0.8)"
    rounded: "{rounded.2xl}"
    padding: 4px

  auth-tab-active:
    backgroundColor: "#ffffff"
    textColor: "{colors.brand-indigo-deep}"
    shadow: "{shadows.sm}"
    fontSize: 12px
    fontWeight: 700
    lineHeight: 16px

  auth-tab-inactive:
    textColor: "{colors.text-tertiary}"
    fontSize: 12px
    fontWeight: 700
    lineHeight: 16px
    hoverTextColor: "{colors.text-primary}"

  form-input:
    backgroundColor: "{colors.input-bg}"
    borderColor: "rgba(226, 232, 240, 0.6)"
    typography: "{typography.body-sm}"
    rounded: "{rounded.xl}"
    padding: "12px 16px"
    focusBorderColor: "{colors.brand-indigo-light}"
    focusBackgroundColor: "#ffffff"

  form-label:
    typography: "{typography.caption}"
    textColor: "{colors.text-muted}"
    marginBottom: 6px

  modal-overlay:
    backgroundColor: "{colors.overlay-bg}"
    backdropFilter: "blur(4px)"
    zIndex: 50

  modal-card:
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.2xl}"
    padding: "{spacing.3xl}"
    shadow: "{shadows.xl}"

  modal-close-button:
    size: 24px
    rounded: "{rounded.lg}"
    backgroundColor: "rgba(248, 250, 252, 1)"
    textColor: "{colors.text-muted}"
    hoverTextColor: "{colors.text-secondary}"

  toast:
    glassEffect: "{glass-effects.toast}"
    shadow: "{shadows.lg}"
    borderColor: "{colors.border-light}"
    rounded: "{rounded.xl}"
    padding: "14px"
    autoDismiss: 3500ms
    pathTypography:
      fontSize: 12px
      fontWeight: 700
      lineHeight: 16px

  toast-method-badge:
    width: 24px
    height: 24px
    rounded: "{rounded.lg}"
    fontSize: 9px
    fontWeight: 700

  decorative-blur-orb:
    dashboard:
      size: 160px
      positions:
        top-right: "-right-10 -top-10"
        bottom-left: "-left-10 -bottom-10"
    auth:
      size: 144px
      positions:
        top-right: "-right-16 -top-16"
        bottom-left: "-left-16 -bottom-16"
    rounded: "{rounded.full}"
    blur: "blur(40px)"
    zIndex: 0

  sidebar-overlay:
    backgroundColor: "{colors.overlay-bg}"
    backdropFilter: "blur(4px)"
    zIndex: 40

  action-button-mini:
    size: 28px
    rounded: "{rounded.lg}"
    backgroundColor: "#ffffff"
    borderColor: "{colors.border-default}"
    textColor: "{colors.text-tertiary}"
    hoverTextColor: "{colors.brand-indigo-deep}"
    hoverBorderColor: "rgba(99, 102, 241, 0.2)"
    shadow: "{shadows.sm}"
    transition: "{transitions.default}"

  action-button-mini-danger:
    hoverTextColor: "{colors.semantic-error-text}"
    hoverBorderColor: "{colors.semantic-error-border}"

  # ─── Examples (illustrative) — auto-derived; resolve any TO_FILL markers below ───
  ex-stats-card:
    description: "Dashboard stats card — re-uses glass-panel + interactive-card hover effect."
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.2xl}"
    padding: "{spacing.2xl}"
    hoverTransform: "translateY(-4px)"
    hoverShadow: "{shadows.card-hover}"
  ex-data-table-cell:
    description: "Default data-table th + td chrome. Header uses caption typography; body uses body-sm-medium."
    headerTypography: "{typography.caption}"
    bodyTypography: "{typography.body-sm-medium}"
    cellPadding: "{spacing.lg} {spacing.3xl}"
    rowBorder: "1px solid rgba(241, 245, 249, 0.6)"
  ex-auth-form-card:
    description: "Sign-in / sign-up card. Re-uses glass-panel with text-input primitives inside."
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.3xl}"
    padding: "{spacing.4xl}"
    maxWidth: 448px
  ex-modal-card:
    description: "Modal dialog surface — same glass-panel chrome as auth-form with elevated shadow."
    glassEffect: "{glass-effects.panel}"
    rounded: "{rounded.2xl}"
    padding: "{spacing.3xl}"
  ex-toast:
    description: "Toast notification surface — glass-toast + medium shadow with auto-dismiss."
    glassEffect: "{glass-effects.toast}"
    rounded: "{rounded.xl}"
    padding: "14px"
    typography: "{typography.body-sm}"
    autoDismiss: 3500ms
  ex-route-card:
    description: "Route architecture card — three color variants mapped to HTTP methods."
    rounded: "{rounded.xl}"
    padding: "10px"
    methodBadgeSize: "48px"
    methodBadgeTypography: "10px / 700"

---


## Overview

WebShelf Admin 采用 **Indigo-Glass 毛玻璃设计语言**，通过三层质感体系营造轻盈、现代、专业的 BFF 管理后台体验：

1. **柔和多色渐变背景** — `#f0f4ff → #fbf8ff → #fff4f4` 的三色过渡奠定温暖基调，替代传统纯白/纯灰后台。
2. **毛玻璃半透明面板** — `rgba(255,255,255,0.7)` + `backdrop-filter: blur(16px)` 构成核心面板质感，让内容层级在柔和背景上自然浮现。
3. **indigo→purple 渐变品牌按钮** — `linear-gradient(135deg, #6366f1 0%, #a855f7 100%)` 作为唯一 CTA 色彩，hover 时上浮 + 发光阴影 + 微提亮。

毛玻璃贯穿 Sidebar（`0.85` 透明度 / `blur(20px)`）、Header、卡片、Modal 全部层级。装饰性模糊光晕（indigo / pink / purple 大圆）散落在 Banner 和 Auth 页面，制造呼吸感而非喧哗。

**Key Characteristics:**
- 毛玻璃 (`backdrop-filter: blur`) 是核心质感，贯穿所有面板组件，**不可移除**。
- 品牌色 indigo `#6366f1` → purple `#a855f7` 渐变仅用于主 CTA 按钮与 Logo 图标。UI 镶边使用 indigo-50 浅色背景。
- 字体族 `Plus Jakarta Sans`（主）+ `Noto Sans SC`（中文后备），权重 300–700 全范围可用。
- `cubic-bezier(0.16, 1, 0.3, 1)` 缓动曲线统一卡片 hover 上浮与按钮动效。
- 卡片 hover 上浮 `translateY(-4px)` + 品牌色 tint 阴影，营造回应感。

## Colors

### 品牌色
- **Indigo** (`{colors.brand-indigo}` — `#6366f1`): 品牌主色，按钮渐变起点，导航激活态。
- **Indigo Light** (`{colors.brand-indigo-light}` — `#818cf8`): 浅 Indigo，输入框/搜索框 focus 边框色。
- **Indigo Deep** (`{colors.brand-indigo-deep}` — `#4f46e5`): 深 Indigo，激活态文字、链接强调。
- **Indigo Dark** (`{colors.brand-indigo-dark}` — `#4338ca`): 最深 Indigo，仅用于渐变延伸。
- **Purple** (`{colors.brand-purple}` — `#a855f7`): 按钮渐变终点，purple 装饰元素。

### 表面/背景
- **Page Gradient** — 三色线性渐变 `135deg, #f0f4ff 0%, #fbf8ff 40%, #fff4f4 100%`。全局页面背景，不可替换为纯色。
- **Glass Panel BG** (`{colors.glass-bg}` — `rgba(255,255,255,0.7)`): 默认毛玻璃面板。
- **Glass Sidebar BG** (`{colors.glass-bg-heavy}` — `rgba(255,255,255,0.85)`): 侧边栏更不透明确保可读性。
- **Glass Toast BG** (`{colors.glass-bg-toast}` — `rgba(255,255,255,0.9)`): Toast 接近实色确保文字清晰。
- **Input BG** (`{colors.input-bg}` — `rgba(255,255,255,0.6)`): 输入框默认背景。
- **Overlay BG** (`{colors.overlay-bg}` — `rgba(15,23,42,0.2)`): Modal / Sidebar 遮罩层。

### 文本层级
- **Text Primary** (`{colors.text-primary}` — `#1e293b`): 标题、重要文本 (slate-800)。
- **Text Secondary** (`{colors.text-secondary}` — `#475569`): 正文、表格内容 (slate-600)。
- **Text Tertiary** (`{colors.text-tertiary}` — `#64748b`): 辅助信息 (slate-500)。
- **Text Muted** (`{colors.text-muted}` — `#94a3b8`): 低优先级标签、占位符 (slate-400)。
- **Text Muted Soft** (`{colors.text-muted-soft}` — `#cbd5e1`): 最轻文本，极细印刷体 (slate-300)。

### 语义色
- **Success** — bg `#ecfdf5` / text `#059669` / border `#a7f3d0` / dot `#10b981`。健康检查通过、在线状态、POST 方法 badge。
- **Warning** — bg `#fffbeb` / text `#d97706` / border `#fde68a`。中间件拦截器状态、PUT 方法 badge。
- **Error** — text `#e11d48` / border `#fecdd3`。DELETE 方法 badge、错误/破坏性操作。

### 装饰色
- **Indigo Soft** — bg `#eef2ff` / border `#e0e7ff`。路由卡片、导航激活态浅色背景。
- **Purple Soft** — bg `#faf5ff` / border `#f3e8ff`。管理员角色 badge。
- **Pink Soft** — bg `#fce7f3` / border `#fce7f3`。装饰光晕。

### 代码控制台
- **Code BG** (`{colors.code-bg}` — `#0f172a`): API 日志流背景 (slate-900)。
- **Code Text** (`{colors.code-text}` — `#cbd5e1`): 默认日志文字 (slate-300)。
- **Code Success** (`{colors.code-success}` — `#34d399`): GET 成功日志行 (emerald-400)。
- **Code Info** (`{colors.code-info}` — `#818cf8`): GET 请求日志行 (indigo-400)。
- **Code Amber** (`{colors.code-amber}` — `#fbbf24`): PUT 请求日志行 (amber-400)。
- **Code Error** (`{colors.code-error}` — `#fb7185`): DELETE / 错误日志行 (rose-400)。

## Typography

### Font Family
主字体 **Plus Jakarta Sans**（权重 300/400/500/600/700），中文后备 **Noto Sans SC**（权重 300/400/500/700）。全栈使用 `font-feature-settings: normal`。等宽字体 `ui-monospace, SFMono-Regular, Menlo, monospace` 仅用于代码控制台。

### Hierarchy

| Token | Size | Weight | Line Height | Letter Spacing | Use |
|---|---|---|---|---|---|
| `{typography.heading-xl}` | 20px | 700 | 28px | 0 | 仪表盘欢迎标题、统计卡片数值。 |
| `{typography.heading-lg}` | 16px | 700 | 24px | 0 | 侧边栏标题、Modal 标题。 |
| `{typography.heading-md}` | 14px | 700 | 20px | 0 | 区域小标题、Avatar 缩写。 |
| `{typography.body-default}` | 14px | 500 | 20px | 0 | 导航项、默认正文。 |
| `{typography.body-sm}` | 12px | 400 | 16px | 0 | 正文、输入框文字、副标题。 |
| `{typography.body-sm-medium}` | 12px | 500 | 16px | 0 | 统计卡片标签、表格内容、Tab 标签。 |
| `{typography.caption}` | 11px | 700 | 16px | 0.05em | 大写分组标签、表头、表单标签。 |
| `{typography.caption-mono}` | 12px | 400 | 16px | 0 | 代码控制台日志。 |
| `{typography.fine-print}` | 10px | 500 | 16px | 0 | 侧边栏副标题、API 路径标注、copyright。 |
| `{typography.fine-print-tiny}` | 9px | 400 | 14px | 0 | Footer 小字。 |
| `{typography.button-label}` | 12px | 600 | 16px | 0 | 所有按钮标签。 |
| `{typography.nav-item}` | 14px | 500 | 20px | 0 | 侧边栏导航链接（=body-default）。 |

### Principles
- **700 weight 用于标题。** 品牌不限制字重天花板，标题使用 700 制造清晰层级。
- **uppercase + tracking 用于元数据。** 侧边栏分组标签、表头、表单标签使用 11px / 700 / 0.05em 大写。
- **Plus Jakarta Sans + Noto Sans SC 双字体栈。** 全站无第三字体族。
- **代码控制台等宽字体。** 仅 API 日志流使用 `caption-mono`。

## Spacing

| Token | Value | Use |
|---|---|---|
| `xs` | 4px | Tab 容器内边距、Badge 内边距。 |
| `sm` | 6px | Badge 水平内边距、间距微调。 |
| `md` | 8px | 元素间常规 gap、小 padding。 |
| `lg` | 12px | 按钮垂直内边距、导航项 padding、表格单元格。 |
| `xl` | 16px | 按钮水平内边距、卡片内边距、输入框 padding、区域 gap。 |
| `2xl` | 20px | 统计卡片 padding。 |
| `3xl` | 24px | 页面区域 padding、Modal padding。 |
| `4xl` | 32px | Auth 表单卡片 padding。 |

## Border Radius

| Token | Value | Use |
|---|---|---|
| `sm` | 2px | (保留，极少用) |
| `md` | 4px | HTTP 方法 badge。 |
| `lg` | 8px | 操作按钮 mini、关闭按钮、Toast 方法 badge。 |
| `xl` | 12px | **按钮、输入框、导航项、路由卡片、Toast** 的标准圆角。 |
| `2xl` | 16px | 统计卡片、Modal 卡片、Tab 容器。 |
| `3xl` | 24px | **毛玻璃面板** (glass-panel)、数据表格容器、Auth 表单卡片。 |
| `full` | 9999px | 在线状态 Badge、Avatar 头像。 |

## Shadows & Depth

| Level | Value | Use |
|---|---|---|
| `xs` | `0 1px 2px 0 rgba(0, 0, 0, 0.03)` | 极小阴影（备用）。 |
| `sm` | `0 1px 2px 0 rgba(0, 0, 0, 0.05)` | 表格容器、操作按钮 mini。 |
| `md` | `0 4px 6px -1px rgba(0,0,0,0.1), 0 2px 4px -2px rgba(0,0,0,0.1)` | 按钮默认阴影。 |
| `lg` | `0 10px 15px -3px rgba(0,0,0,0.1), 0 4px 6px -4px rgba(0,0,0,0.1)` | Toast 通知。 |
| `xl` | `0 20px 25px -5px rgba(0,0,0,0.1), 0 8px 10px -6px rgba(0,0,0,0.1)` | Auth 表单卡片。 |
| `brand-hover` | `0 10px 20px -5px rgba(99,102,241,0.4)` | 按钮 hover 发光。 |
| `card-hover` | `0 20px 25px -5px rgba(99,102,241,0.08), 0 10px 10px -5px rgba(99,102,241,0.03)` | 卡片 hover 品牌 tint 阴影。 |
| `inner` | `inset 0 2px 4px 0 rgba(0,0,0,0.04)` | 代码控制台嵌入感。 |

### Easing Curves

| Token | Value | Use |
|---|---|---|
| `card` | `cubic-bezier(0.16, 1, 0.3, 1)` 0.3s | 卡片 hover 上浮。 |
| `button` | `cubic-bezier(0.16, 1, 0.3, 1)` 0.4s | 按钮 hover 动效。 |
| `nav` | `ease` 0.2s | 导航项切换。 |
| `default` | `ease` 0.2s | 通用过渡。 |

## Glass Effects

毛玻璃是本设计系统的**核心质感**，不可降级为纯色面板。

| Variant | Background | Blur | Border | Use |
|---|---|---|---|---|
| `panel` | `rgba(255,255,255,0.7)` | `blur(16px)` | `1px solid rgba(255,255,255,0.5)` | Header、统计卡片、Modal、Auth 表单、数据表格 |
| `sidebar` | `rgba(255,255,255,0.85)` | `blur(20px)` | `1px solid rgba(226,232,240,0.8)` right | 侧边栏 |
| `toast` | `rgba(255,255,255,0.9)` | `blur(12px)` | `1px solid rgba(241,245,249,0.8)` | Toast 通知 |

**注意**：需同时提供 `-webkit-backdrop-filter` 前缀以兼容 Safari。

## Decorative Blur Orbs

大型模糊圆形装饰，散布在 Banner 和 Auth 页面背景中，制造呼吸感。

| Variant | Color | Use |
|---|---|---|
| `indigo-soft` | `rgba(199,210,254,0.2)` 转 `blur(40px)` | Dashboard Banner 右上角 |
| `pink-soft` | `rgba(251,207,232,0.2)` 转 `blur(40px)` | Dashboard Banner 左下角 |
| `indigo-medium` | `rgba(199,210,254,0.3)` 转 `blur(40px)` | Auth 页面左下角 |
| `purple-soft` | `rgba(233,213,255,0.3)` 转 `blur(40px)` | Auth 页面右上角 |

规格：
- Dashboard orbs：`w-40 h-40` (160px)，位置 `-right-10 -top-10` / `-left-10 -bottom-10`
- Auth orbs：`w-36 h-36` (144px)，位置 `-right-16 -top-16` / `-left-16 -bottom-16`
- 通用：`rounded-full`，绝对定位，`z-0`，不可交互。



## Layout Architecture

### 整体结构

```
┌──────────────────────────────────────────────────────┐
│  AppShell (min-h-screen, flex, page-gradient)        │
│  ┌────────────┬─────────────────────────────────────┐│
│  │ Sidebar    │  Main Area (flex-1, h-screen,       ││
│  │ (w-64,     │   overflow-y-auto)                  ││
│  │  256px)    │  ┌───────────────────────────────┐  ││
│  │            │  │  TopHeader (h-20, sticky,      │  ││
│  │  glass-    │  │   glass-panel, z-30)           │  ││
│  │  sidebar   │  ├───────────────────────────────┤  ││
│  │  fixed     │  │                                │  ││
│  │  z-40      │  │  Content Area (max-w-7xl,      │  ││
│  │            │  │   mx-auto, p-6, space-y-6)     │  ││
│  │  Nav       │  │  ┌──────────────────────────┐  │  ││
│  │  Links     │  │  │ View: Dashboard / Users / │  │  ││
│  │            │  │  │ Auth                      │  │  ││
│  │  Footer    │  │  └──────────────────────────┘  │  ││
│  └────────────┘  └───────────────────────────────┘  ││
└──────────────────────────────────────────────────────┘
```

**规格：**
- AppShell: `min-height: 100vh; display: flex;` 背景为三色 page-gradient。
- Sidebar: 固定宽度 256px（`w-64`），`glass-sidebar` 毛玻璃，桌面端常驻（`md:translate-x-0`），移动端 `-translate-x-full` overlay 抽屉模式。
- TopHeader: 高度 80px（`h-20`），`glass-panel` 毛玻璃，`sticky top-0 z-30`，底边 `1px solid rgba(241,245,249,0.6)`。
- Content Area: `max-width: 1280px`（`max-w-7xl`），水平居中 `mx-auto`，padding `24px`（`p-6`），子元素间距 `24px`（`space-y-6`）。

### 响应式断点

原型使用 Tailwind CSS 默认断点（`sm:640px` / `md:768px` / `lg:1024px`），行为分层如下：

| 名称 | 宽度 | 断点 | 关键变化 |
|---|---|---|---|
| Mobile | < 640px | — | 所有网格 1-up；Sidebar overlay 隐藏。 |
| Tablet | 640–1023px | `sm` / `md` | 统计卡片 2-up（`sm:`）；搜索框可见（`sm:`）；Sidebar 可切换（`md:`）。 |
| Desktop | ≥ 1024px | `lg` | 统计卡片 4-up；双栏 3:1 布局；Sidebar 常驻（`md:translate-x-0`）。 |

### 侧边栏响应式行为
- 桌面端 (`md`+): `translate-x-0`，静态置于左侧，不遮挡内容。
- 移动端 (`< md`): `-translate-x-full`，通过汉堡菜单按钮触发 `toggleSidebar()`，展开时叠加半透明遮罩 `sidebar-overlay`。
- 移动端视图切换时自动收起侧边栏。

---

## Dioxus 组件清单与规格

以下组件需在 `app/ui/src/` 中实现为 Dioxus component。每个组件配套一个 `assets/styling/<name>.css` 样式文件。

### 3.1 AppShell

**文件**：`src/app_shell.rs` + `assets/styling/app_shell.css`

根布局组件。

```rust
#[component]
pub fn AppShell(sidebar: Element, children: Element) -> Element
```

**设计规格**：
- `display: flex; min-height: 100vh;`
- 背景：三色 page-gradient `linear-gradient(135deg, #f0f4ff 0%, #fbf8ff 40%, #fff4f4 100%)`
- 无 padding，无边距

### 3.2 Sidebar

**文件**：`src/sidebar.rs` + `assets/styling/sidebar.css`

**设计规格**：
- 宽度：`w-64` (256px)，`fixed inset-y-0 left-0 z-40`
- 毛玻璃：`glass-sidebar`（`rgba(255,255,255,0.85)` + `blur(20px)` + 右侧 `1px solid rgba(226, 232, 240, 0.8)` 分割线）
- 移动端：`-translate-x-full`，通过 `toggleSidebar()` 切换，带 `overlay` 遮罩
- Logo 区域：高 80px（`h-20`），`px-6`，底部 `border-b border-slate-100`
  - 图标容器：`w-9 h-9 rounded-xl bg-gradient-to-tr from-indigo-500 to-purple-500`，内含 Font Awesome `fa-layer-group` 白色图标，`shadow-md shadow-indigo-200`
  - 标题：`{typography.heading-lg}`，`bg-gradient-to-r from-indigo-600 to-purple-600 bg-clip-text text-transparent`（渐变色文字），文字 "WebShelf Admin"
  - 副标题：`{typography.fine-print}`，`{colors.text-muted}`，`tracking-wider uppercase`，文字 "Rust Fullstack Framework"
- 导航分组标签：`{typography.caption}`，`{colors.text-muted}`，`px-3 mb-2`
- 导航项：
  - 默认态：`{typography.nav-item}`，`{colors.text-secondary}`，`px-3 py-2.5 rounded-xl`，`hover:bg-slate-50 hover:text-slate-900`
  - 激活态：`bg-indigo-50/60` + `{colors.brand-indigo-deep}`（不要全背景反色）
- 导航结构（对齐 prototype.html）：
  ```
  [核心系统监控]              ← caption (uppercase)
    · 控制中心 (/health)      ← nav-item
  [数据管理]  [admin_layer]   ← caption + amber-compact badge
    · 用户管理 (/users)       ← nav-item
  [网关鉴权演示]              ← caption
    · 身份认证门禁             ← nav-item
  ```
- 底部 Footer：
  - GitHub 链接卡片：`p-2.5 bg-white border border-slate-100 rounded-xl shadow-sm`，hover `border-indigo-200`
  - 版权信息：`{typography.fine-print}` / `{typography.fine-print-tiny}`，`{colors.text-muted}`

### 3.3 TopHeader

**文件**：`src/top_header.rs` + `assets/styling/top_header.css`

**设计规格**：
- 高度：80px（`h-20`），`sticky top-0 z-30`
- 毛玻璃：`glass-panel`（`rgba(255,255,255,0.7)` + `blur(16px)`），底边 `1px solid rgba(241,245,249,0.6)`
- 左侧：移动端汉堡菜单按钮（`md:hidden`，点击 `toggleSidebar()`）+ 搜索框
  - 搜索框：`bg-slate-100/80 px-3 py-1.5 rounded-xl border border-slate-200/40`，`w-64`，focus 态 `border-indigo-400 bg-white`
  - 搜索框文字：`{typography.body-sm}`，`{colors.text-secondary}`，placeholder "搜索资源与 API 端点..."
- 右侧：
  - 在线状态 badge：`inline-flex px-2 py-0.5 rounded-full`，`bg-emerald-50 text-emerald-600 border border-emerald-200/50`，带 `emerald-500` 圆点 `animate-pulse`，文字 `11px / 600`（`text-[11px] font-semibold`），非大写
  - 分隔线：`h-8 w-[1px] bg-slate-200/60`
  - Avatar：`w-10 h-10 rounded-full bg-gradient-to-tr from-purple-100 to-indigo-100 border border-white shadow-inner`，文字 `{typography.heading-md} text-indigo-600`，"WS"
  - 用户名 + 邮箱：`{typography.body-sm}` bold + `{typography.fine-print}` muted

### 3.4 Button

**文件**：`src/button.rs` + `assets/styling/button.css`

唯一主 CTA 变体：

| 属性 | 值 |
|---|---|
| 背景 | `{colors.brand-gradient}` |
| 文字色 | `#ffffff` |
| 字体 | `{typography.button-label}` |
| 圆角 | `{rounded.xl}` (12px) |
| Padding | `10px 16px`（垂直 10px，水平 16px） |
| 阴影 | `{shadows.md}` |
| 过渡 | `{transitions.button}` |
| hover | `translateY(-2px)` + `{shadows.brand-hover}` + `filter: brightness(1.05)` |

**注**：原型中仅此一种按钮样式（`.btn-gradient`）。暂不需要 secondary / text-arrow / icon-circular 变体。

### 3.5 Badge

**文件**：`src/badge.rs` + `assets/styling/badge.css`

| 变体 | 背景 | 文字色 | 边框 |
|---|---|---|---|
| `success` | `{colors.semantic-success-bg}` | `{colors.semantic-success-text}` | `{colors.semantic-success-border}` |
| `warning` | `{colors.semantic-warning-bg}` | `{colors.semantic-warning-text}` | `{colors.semantic-warning-border}` |
| `admin` | `{colors.accent-purple-soft-bg}` | `#9333ea` | `#f3e8ff` |
| `user` | `#f1f5f9` | `{colors.text-secondary}` | `rgba(226,232,240,0.6)` |
| `amber-compact` | `{colors.semantic-warning-bg}` | `{colors.semantic-warning-text}` | `rgba(253, 230, 138, 0.6)` |

**规格**：
- 字体：`10px / 600 / normal`（`text-[10px] font-semibold`）
- 圆角：`{rounded.base}`（6px，对应原型 Tailwind 的 `rounded-md` 圆角）
- Padding：`2px 8px`（`py-0.5 px-2`）
- `amber-compact` 变体字体：`9px / 600 / normal`（用于 Sidebar 中的 `admin_layer` 标签）

### 3.6 TextInput

**文件**：`src/text_input.rs` + `assets/styling/text_input.css`

**规格**：
- 背景：`{colors.input-bg}` (`rgba(255,255,255,0.6)`)
- 文字色：`{colors.text-primary}`
- 边框：`1px solid rgba(226,232,240,0.6)`
- 字体：`{typography.body-sm}` (12px / 400)
- 圆角：`{rounded.xl}` (12px)
- Padding：`12px 16px`
- focus 态：`border-color: {colors.brand-indigo-light}` + `background: #ffffff`
- 标签文字：`{typography.caption}`（11px / 700 / uppercase），颜色 `{colors.text-muted}`，`mb-1.5`
- **Modal 变体**：背景 `bg-white`（非 `white/60`），边框 `border-slate-200`（非 `slate-200/60`），padding `8px 12px`（`px-3 py-2`，比 Auth 表单更紧凑）

### 3.7 Modal

**文件**：`src/modal.rs` + `assets/styling/modal.css`

**规格**：
- 遮罩层：`fixed inset-0 z-50 bg-slate-900/20 backdrop-blur-sm`
- 卡片：`glass-panel` 毛玻璃，`{rounded.2xl}` (16px)，`p-6` (24px)，`max-w-md`
- 标题：`{typography.heading-md}`，`{colors.text-primary}`
- 关闭按钮：`w-6 h-6 rounded-lg bg-slate-50`，`{colors.text-muted}`，hover `{colors.text-secondary}`
- 表单使用 TextInput 组件（**Modal 变体**：`bg-white`、`border-slate-200`、`px-3 py-2`）
- 提交按钮使用 Button 组件，全宽（Modal 中垂直 padding 为 `py-2.5`=10px）

### 3.8 StatsCard

**文件**：`src/stats_card.rs` + `assets/styling/stats_card.css`

Dashboard 统计卡片（健康度、用户数、接口耗时、中间件状态）。

**规格**：
- 毛玻璃：`glass-panel`
- 圆角：`{rounded.2xl}` (16px)
- Padding：`p-5` (20px)
- 过渡：`{transitions.card}`
- hover：`translateY(-4px)` + `{shadows.card-hover}` + `border-color: rgba(99,102,241,0.2)`
- 布局：`flex items-center justify-between`
- 图标：`w-12 h-12 rounded-xl` 语义色 10% 背景 + 语义色图标

**语义色映射**：

| 卡片 | 图标背景 | 图标色 |
|---|---|---|
| 服务健康度 | `bg-emerald-50` | `text-emerald-500` |
| 管控用户数 | `bg-indigo-50` | `text-indigo-500` |
| 接口平均耗时 | `bg-purple-50` | `text-purple-500` |
| 中间件拦截器 | `bg-amber-50` | `text-amber-500` |

- 标签：`{typography.body-sm-medium}`，`{colors.text-muted}`
- 数值：`{typography.heading-xl}`，默认 `{colors.text-primary}`
  - 健康卡数值使用 `{colors.semantic-success-text}`（`text-emerald-600`）
  - 中间件卡数值使用 `{colors.semantic-warning-text}`（`text-amber-600`）

### 3.9 DataTable

**文件**：`src/data_table.rs` + `assets/styling/data_table.css`

**规格**：
- 容器：`glass-panel` 毛玻璃，`{rounded.3xl}` (24px)，`{shadows.sm}`，`border border-slate-100`
- 表头：`{typography.caption}`（11px / 700 / uppercase / 0.05em），`{colors.text-muted}`，`bg-slate-50/70`
- 表头底部分割线：`border-b border-slate-100`（100% 不透明度，与行分割线区分）
- 表体：`{typography.body-sm-medium}` (12px / 500)，`{colors.text-secondary}`
- 行分割：`border-b border-slate-100/60`
- 行 hover：`bg-slate-50/40`
- 表头单元格 padding：`py-4 px-6`（16px 垂直）
- 表体单元格 padding：`py-3.5 px-6`（14px 垂直）
- 角色列使用 Badge 组件（admin → purple soft, user → slate）
- 时间列（实例孵化时间）：`{typography.caption-mono}` (12px monospace)，`{colors.text-muted}`
- 操作列：两个 `action-button-mini`（编辑、删除），opacity-80，group-hover:opacity-100

### 3.10 AuthForm

**文件**：`src/auth_form.rs` + `assets/styling/auth_form.css`

**规格**：
- 外层居中：`flex items-center justify-center`
- 卡片：`glass-panel` 毛玻璃，`{rounded.3xl}` (24px)，`p-8` (32px)，`shadow-xl shadow-purple-100/30`，`max-w-md w-full`
- 装饰光晕：右上 `purple-soft` orb（144px，`-right-16 -top-16`）+ 左下 `indigo-medium` orb（144px，`-left-16 -bottom-16`）
- Tab 切换：
  - 容器：`bg-slate-100/80 rounded-2xl p-1`
  - 激活 tab：`bg-white shadow-sm text-indigo-600`，`12px / 700`（`text-xs font-bold`）
  - 未激活 tab：`text-slate-500`，hover `text-slate-800`，`12px / 700`（`text-xs font-bold`）
- 表单字段：使用 TextInput 组件
- checkbox + "维持持久化登录"：`text-[11px] text-slate-500`，checkbox `rounded border-slate-300 text-indigo-600`
- 提交按钮：使用 Button 组件，全宽，`mt-6`，垂直 padding `py-3`=12px
- "忘记凭证?" 链接：`11px / 500`，`text-indigo-600`，hover `underline`

### 3.11 Toast

**文件**：`src/toast.rs` + `assets/styling/toast.css`

**规格**：
- 容器：`fixed top-6 right-6 z-50 flex flex-col gap-3 max-w-sm`
- 单项：`glass-toast`（`rgba(255,255,255,0.9)` + `blur(12px)`），`{rounded.xl}` (12px)，`p-3.5` (14px)，`{shadows.lg}`
- 入场动效：`translate-y-2 opacity-0` → `translate-y-0 opacity-100`（50ms 后触发）
- 消失动效：`opacity-0 scale-95`（3s 后触发），300ms 后 remove
- 左侧方法 badge：`w-6 h-6 rounded-lg` 对应 HTTP 方法色
  - GET → `bg-indigo-500`
  - POST → `bg-emerald-500`
  - PUT → `bg-amber-500`
  - DELETE → `bg-rose-500`
- 文字：路径 `12px / 700`（`text-xs font-bold`）+ 状态 `{typography.fine-print}`

### 3.12 CodeConsole

**文件**：`src/code_console.rs` + `assets/styling/code_console.css`

API 日志流模拟终端。

**头部区域规格**：
- 标题：`{typography.heading-md}` (14px / 700)，`{colors.text-primary}`，文字 "服务链路追踪监控"
- 副标题：`{typography.caption}` (11px)，`{colors.text-muted}`，文字 "实时拦截捕获的 WebShelf Axum 后端路由流量"
- Live Stream 标签：`{typography.fine-print}` (10px)，`font-family: {typography.font-mono}`，`background: #f1f5f9`，`{colors.text-muted}`，`padding: 2px 8px`，`{rounded.md}`

**规格**：
- 背景：`{colors.code-bg}` (`#0f172a`)
- 圆角：`{rounded.xl}` (12px)
- Padding：`p-4`
- 阴影：`{shadows.inner}`
- 字体：`{typography.caption-mono}` (12px monospace)
- 文字色：`{colors.code-text}` (slate-300)
- 日志行带彩色圆点指示器（`w-1.5 h-1.5 rounded-full`），颜色对应方法：
  - GET/成功 → `bg-emerald-400`
  - GET/信息 → `bg-indigo-400`
  - PUT → `bg-amber-400`
  - DELETE/错误 → `bg-rose-400`
- 滚动：`overflow-y-auto`，隐藏滚动条（`.no-scrollbar`）

### 3.13 RouteCard

**文件**：`src/route_card.rs` + `assets/styling/route_card.css`

路由架构图中的单条路由展示。

**规格**：
- 三种颜色变体，分别对应不同的 HTTP 方法路由：
  - **indigo 变体**（POST）：背景 `bg-indigo-50/50`，边框 `border border-indigo-100`
  - **purple 变体**（GET）：背景 `bg-purple-50/50`（`rgba(250,245,255,0.5)`），边框 `border-purple-100`（`#f3e8ff`）
  - **pink 变体**（PUT）：背景 `bg-pink-50/50`（`rgba(253,242,248,0.5)`），边框 `border-pink-100`（`#fce7f3`）
- 圆角：`{rounded.xl}` (12px)
- Padding：`p-2.5`
- HTTP 方法 badge：`w-12 text-center`，`10px / 700`（`text-[10px] font-bold`），`{rounded.md}` (4px)
  - POST → `bg-indigo-500 text-white`
  - GET → `bg-purple-500 text-white`
  - PUT → `bg-rose-500 text-white`
- 路径：`{typography.body-sm}` bold，`{colors.text-primary}`
- 描述：`{typography.fine-print}`，`{colors.text-muted}`

---

## 视图级布局 (View Layouts)

### 4.1 Dashboard 视图

对应 prototype.html `view-dashboard`。

```
┌────────────────────────────────────────────┐
│  Hero Banner (glass-panel, rounded-3xl)     │
│  ┌──────────────────────────────────────┐  │
│  │ ← indigo orb (blur, -right -top)     │  │
│  │ ← pink orb (blur, -left -bottom)     │  │
│  │  欢迎标题 (heading-xl, text-primary)  │  │
│  │  副文本 (body-sm, text-muted)         │  │
│  │                     [健康检查按钮]    │  │
│  └──────────────────────────────────────┘  │
├────────────────────────────────────────────┤
│  统计卡片行 (grid-cols-1 sm:2 lg:4)        │
│  [StatsCard] [StatsCard]                    │
│  [StatsCard] [StatsCard]                    │
├────────────────────────────────────────────┤
│  双栏区域 (grid-cols-1 lg:grid-cols-3)      │
│  ┌──────────────────────┬───────────────┐  │
│  │ CodeConsole          │ 路由架构图    │  │
│  │ (lg:col-span-2,      │ (glass-panel, │  │
│  │  h-[340px],          │  rounded-2xl, │  │
│  │  code-bg 终端)       │  h-[340px])   │  │
│  │                      │  RouteCard × 3│  │
│  └──────────────────────┴───────────────┘  │
└────────────────────────────────────────────┘
```

**Hero Banner 规格**：
- 毛玻璃：`glass-panel`，`rounded-3xl`，`p-6`，`border border-indigo-100/40 shadow-sm shadow-indigo-50`（indigo 色调边框与阴影）
- 装饰光晕：右上 `indigo-soft` orb（`-right-10 -top-10`）+ 左下 `pink-soft` orb（`-left-10 -bottom-10`）
- 标题：`{typography.heading-xl}` (20px / 700)，`{colors.text-primary}`
- 副标题：`{typography.body-sm}` (12px)，`{colors.text-tertiary}` (`text-slate-500`)
- CTA 按钮：Button 组件，右侧对齐

### 4.2 Users 视图

对应 prototype.html `view-users`。

```
┌────────────────────────────────────────────┐
│  [标题 (heading-xl) + admin badge]         │
│             [创建新用户 Button 右对齐]      │
│  副标题 (body-sm, text-muted)              │
├────────────────────────────────────────────┤
│  DataTable (glass-panel, rounded-3xl)      │
│  ID │ 账户身份 │ 安全邮箱 │ 授权标签 │ 实例孵化时间 │操作 │
├────┼─────────┼─────────┼─────────┼────────────┼────┤
│  1  │  admin  │  ...    │  管理员  │ 2026-06-01 │ ✎🗑 │
│  2  │  ...    │  ...    │ 普通用户 │ 2026-06-05 │ ✎🗑 │
└────────────────────────────────────────────┘
```

**规格**：
- 标题：`{typography.heading-xl}` (20px / 700)，`{colors.text-primary}`
- 管理员区域 tag：`text-xs font-semibold bg-indigo-50 text-indigo-600 px-2 py-0.5 rounded-full border border-indigo-100`，显示 "require_admin 中间件保护区域"
- "创建新用户"按钮：Button 组件
- 表格：DataTable 组件
- 时间列（实例孵化时间）：`{typography.caption-mono}`，`{colors.text-muted}`

### 4.3 Auth 视图

对应 prototype.html `view-auth`。

```
┌────────────────────────────────────────────┐
│            ┌─────────────────┐             │
│            │ ← orb (purple)  │             │
│            │  [登录] [注册]   │  Tab 切换   │
│            │                  │             │
│            │  邮箱输入框      │             │
│            │  密码输入框      │             │
│            │  [ ]保持登录     │             │
│            │                  │             │
│            │  [提交 (全宽)]   │             │
│            │ ← orb (indigo)  │             │
│            └─────────────────┘             │
│              AuthForm                      │
└────────────────────────────────────────────┘
```

**规格**：参见 AuthForm 组件规格（3.10）。

---

## 原型的 CSS 特效 → 设计令牌的直接映射

prototype.html 中的特效**原样保留**并 token 化：

| 原型特效 | Token 映射 | 说明 |
|---|---|---|
| 三色渐变页面背景 (`linear-gradient(135deg, #f0f4ff...)`) | `{colors.page-gradient}` | 全局背景，不可替换为纯色。 |
| `.glass-panel` (`rgba(255,255,255,0.7)` + `blur(16px)`) | `{glass-effects.panel}` | Header、卡片、Modal、数据表格的默认面板质感。 |
| `.glass-sidebar` (`rgba(255,255,255,0.85)` + `blur(20px)`) | `{glass-effects.sidebar}` | 侧边栏专用，更高不透明度确保导航文字可读。 |
| `.btn-gradient` (`indigo→purple` + hover 上浮发光) | `{components.button-primary}` | 唯一 CTA 按钮。 |
| `.interactive-card` (hover 上浮 + 品牌阴影) | `{transitions.card}` + `{shadows.card-hover}` | 统计卡片 hover 效果。 |
| 装饰模糊光晕 (`blur-2xl` 圆形) | `{decorative-blurs.*}` | Dashboard Banner 和 Auth 页面的氛围装饰，**保留**。 |
| `Plus Jakarta Sans` + `Noto Sans SC` | `{typography.font-family}` | 主字体族，权重 300–700 全范围可用。 |
| 侧边栏抽屉（`translateX` + overlay 遮罩） | Sidebar 组件 | 保留交互逻辑，遮罩 `bg-slate-900/20 backdrop-blur-sm`。 |
| Font Awesome 图标 | CDN 引入 | 保留使用，图标色对应语义色。 |
| Toast 通知系统 | Toast 组件 | 保留入场/消失动效与自动销毁逻辑。 |
| 隐藏滚动条 `.no-scrollbar` | utility class | 保留用于 Sidebar 和 CodeConsole。 |
| `h-20` header / sidebar logo | 80px | **不缩减**为 64px，维持原型比例。 |

---

## 文件组织规范

`app/ui/src/` 目标结构：

```
src/
├── lib.rs                 # crate 入口，pub mod + pub use
├── tokens.rs              # 设计令牌常量 (colors / typography / glass / spacing)
├── app_shell.rs           # AppShell 根布局
├── sidebar.rs             # Sidebar 侧边栏
├── top_header.rs          # TopHeader 顶栏
├── button.rs              # Button 按钮
├── badge.rs               # Badge 标签
├── text_input.rs          # TextInput 输入框
├── modal.rs               # Modal 模态框
├── stats_card.rs          # StatsCard 统计卡片
├── data_table.rs          # DataTable 数据表格
├── auth_form.rs           # AuthForm 认证表单
├── toast.rs               # Toast 通知
├── code_console.rs        # CodeConsole 代码控制台
├── route_card.rs          # RouteCard 路由卡片
├── hero.rs                # （现有，按本设计系统重构）
├── navbar.rs              # （现有，按本设计系统重构或合并到 sidebar）
└── views/
    ├── mod.rs
    ├── dashboard.rs       # Dashboard 视图
    ├── users.rs           # Users 管理视图
    └── auth.rs            # Auth 认证视图
```

`assets/styling/` 下每个组件对应一个 CSS 文件。

---

## Examples (illustrative)

> Auto-derived demonstration surfaces. Each `ex-*` entry references brand-native primitives so downstream consumers re-skin the same surfaces consistently.

**`ex-stats-card`** — Dashboard stats card. Re-uses `glass-panel` with `interactive-card` hover effect.
- Properties: `glassEffect`, `rounded`, `padding`, `hoverTransform`, `hoverShadow`

**`ex-data-table-cell`** — Default data-table th + td chrome. Header uses `caption` typography (11px / 700 / uppercase / 0.05em); body uses `body-sm-medium` (12px / 500).
- Properties: `headerTypography`, `bodyTypography`, `cellPadding`, `rowBorder`

**`ex-auth-form-card`** — Sign-in / sign-up card. Re-uses `glass-panel` with `text-input` primitives inside.
- Properties: `glassEffect`, `rounded`, `padding`, `maxWidth`

**`ex-modal-card`** — Modal dialog surface — same `glass-panel` chrome with elevated shadow.
- Properties: `glassEffect`, `rounded`, `padding`

**`ex-toast`** — Toast notification surface — `glass-toast` + medium shadow with 3500ms auto-dismiss.
- Properties: `glassEffect`, `rounded`, `padding`, `typography`, `autoDismiss`

**`ex-route-card`** — Route architecture card — three color variants (indigo / purple / pink) mapped to HTTP methods (POST / GET / PUT).
- Properties: `rounded`, `padding`, `methodBadgeSize`, `methodBadgeTypography`

---

## 关键约束 (Do's & Don'ts)

### ✅ Do
- **毛玻璃是核心质感。** 所有面板组件（Header、Sidebar、Card、Modal、DataTable）使用 `glass-effects.*` 的 `backdrop-filter: blur()`。
- **三色渐变作为全局页面背景。** 不可降级为纯白或纯色。
- **Plus Jakarta Sans + Noto Sans SC 双字体族。** 权重 300/400/500/600/700 全范围可用。
- **indigo→purple 渐变仅用于主 CTA 按钮和 Logo 图标。**
- **装饰模糊光晕保留。** Dashboard Banner 和 Auth 页面的 `blur-2xl` 圆形不可移除。
- **`cubic-bezier(0.16, 1, 0.3, 1)` 缓动曲线统一所有卡片/按钮动效。**
- **卡片 hover 上浮 `translateY(-4px)` + 品牌 tint 阴影。**
- **表头/标签使用 uppercase + tracking 的 `caption` 规格。**
- **隐藏滚动条但保留滚动功能**（Sidebar、CodeConsole）。

### ❌ Don't
- **不要移除毛玻璃。** `backdrop-filter: blur()` 是本系统的根基。
- **不要用纯色面板替代玻璃面板。**
- **不要把页面背景换成纯白。** 三色渐变是页面基调。
- **不要移除装饰模糊光晕。** 它们是氛围的一部分。
- **不要限制字重天花板为 600。** 标题可使用 700。
- **不要给按钮添加 secondary/text-arrow/icon-circular 变体**（除非后续原型更新引入）。
- **不要缩减 Header/Sidebar logo 高度**（保持 80px / `h-20`）。
- **不要在 Sidebar 激活态使用全背景反色**（使用 `bg-indigo-50/60` 浅色）。
- **不要引入色阶强调色体系**（`accent-purple/pink/blue/orange/green`）。

---

## 设计 Token 的 Rust/CSS 落地方式

在 `app/ui/src/tokens.rs` 中集中定义设计常量：

```rust
// tokens.rs —— 从 prototype.html 提取的设计令牌

pub mod color {
    // 品牌色
    pub const BRAND_INDIGO: &str      = "#6366f1";
    pub const BRAND_INDIGO_DEEP: &str = "#4f46e5";
    pub const BRAND_INDIGO_DARK: &str = "#4338ca";
    pub const BRAND_INDIGO_LIGHT: &str = "#818cf8";
    pub const BRAND_PURPLE: &str      = "#a855f7";

    // 文本层级
    pub const TEXT_PRIMARY: &str    = "#1e293b";
    pub const TEXT_SECONDARY: &str   = "#475569";
    pub const TEXT_TERTIARY: &str    = "#64748b";
    pub const TEXT_MUTED: &str       = "#94a3b8";

    // 表面/玻璃
    pub const GLASS_BG: &str         = "rgba(255, 255, 255, 0.7)";
    pub const GLASS_BG_HEAVY: &str   = "rgba(255, 255, 255, 0.85)";
    pub const GLASS_BG_TOAST: &str   = "rgba(255, 255, 255, 0.9)";
    pub const INPUT_BG: &str         = "rgba(255, 255, 255, 0.6)";
    pub const OVERLAY_BG: &str       = "rgba(15, 23, 42, 0.2)";

    // 语义色
    pub const SUCCESS_BG: &str       = "#ecfdf5";
    pub const SUCCESS_TEXT: &str     = "#059669";
    pub const WARNING_BG: &str       = "#fffbeb";
    pub const WARNING_TEXT: &str     = "#d97706";
    pub const ERROR_TEXT: &str       = "#e11d48";

    // 装饰色
    pub const INDIGO_SOFT_BG: &str   = "#eef2ff";
    pub const PURPLE_SOFT_BG: &str   = "#f3e8ff";

    // 代码控制台
    pub const CODE_BG: &str          = "#0f172a";
    pub const CODE_TEXT: &str        = "#cbd5e1";
    pub const CODE_SUCCESS: &str     = "#34d399";
    pub const CODE_INFO: &str        = "#818cf8";
    pub const CODE_AMBER: &str       = "#fbbf24";
    pub const CODE_ERROR: &str       = "#fb7185";
}

pub mod typography {
    pub const FONT_FAMILY: &str = "Plus Jakarta Sans, Noto Sans SC, sans-serif";
    pub const FONT_MONO: &str   = "ui-monospace, SFMono-Regular, Menlo, monospace";
}

pub mod glass {
    pub const PANEL_BG: &str             = "rgba(255, 255, 255, 0.7)";
    pub const PANEL_BLUR: &str           = "blur(16px)";
    pub const PANEL_BORDER: &str         = "1px solid rgba(255, 255, 255, 0.5)";
    pub const SIDEBAR_BG: &str           = "rgba(255, 255, 255, 0.85)";
    pub const SIDEBAR_BLUR: &str         = "blur(20px)";
    pub const SIDEBAR_BORDER: &str       = "1px solid rgba(226, 232, 240, 0.8)";
    pub const TOAST_BG: &str             = "rgba(255, 255, 255, 0.9)";
    pub const TOAST_BLUR: &str           = "blur(12px)";
}
```

CSS 中通过 `:root { --color-brand-indigo: #6366f1; }` 等自定义属性落地同一套 token，确保 Rust 端和 CSS 端引用一致。

---

*本文档即 `app/ui` crate 的 UI 设计权威提示词。所有 Dioxus 组件的视觉实现必须严格对齐本文档的设计 token 与组件规格。事实源为 `prototype.html`。*
