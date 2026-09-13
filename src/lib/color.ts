/** 验证 hex 颜色字符串是否为合法格式 */
function isValidHex(hex: string): boolean {
  return /^#?[0-9a-fA-F]{6}$/.test(hex);
}

/** 解析 hex 为 [r, g, b]，非法输入返回 [0, 0, 0] */
export function hexToRgb(hex: string): [number, number, number] {
  if (!isValidHex(hex)) {
    console.warn(`[color] 非法 hex 颜色值: "${hex}"，回退到 #000000`);
    return [0, 0, 0];
  }
  const c = hex.replace("#", "");
  return [
    Number.parseInt(c.slice(0, 2), 16),
    Number.parseInt(c.slice(2, 4), 16),
    Number.parseInt(c.slice(4, 6), 16),
  ];
}

/** 将 hex 颜色字符串 + 透明度转 rgba */
export function hexToRgba(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** 将颜色向白色方向提亮 (0~1) */
export function lightenColor(hex: string, amount: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgb(${Math.round(r + (255 - r) * amount)}, ${Math.round(g + (255 - g) * amount)}, ${Math.round(b + (255 - b) * amount)})`;
}

/** 将颜色向黑色方向加深 (0~1) */
export function darkenColor(hex: string, amount: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgb(${Math.round(r * (1 - amount))}, ${Math.round(g * (1 - amount))}, ${Math.round(b * (1 - amount))})`;
}

/** 混合两个 hex 颜色 */
export function blendColors(hex1: string, hex2: string, ratio: number): string {
  const [r1, g1, b1] = hexToRgb(hex1);
  const [r2, g2, b2] = hexToRgb(hex2);
  return `rgb(${Math.round(r1 * ratio + r2 * (1 - ratio))}, ${Math.round(g1 * ratio + g2 * (1 - ratio))}, ${Math.round(b1 * ratio + b2 * (1 - ratio))})`;
}

/** 将强调色与背景色混合，生成气泡色 */
export function accentToBubble(accent: string, blend: number, bgColor: string): string {
  const [ar, ag, ab] = hexToRgb(accent);
  const [br, bg, bb] = hexToRgb(bgColor);
  const r = Math.round(ar * blend + br * (1 - blend));
  const g = Math.round(ag * blend + bg * (1 - blend));
  const b = Math.round(ab * blend + bb * (1 - blend));
  return `rgb(${r}, ${g}, ${b})`;
}

/** 根据背景色亮度派生整套主题 CSS 变量。themeMode 可覆盖自动检测 */
export function applyThemeVars(
  bgColor: string,
  accentColor: string,
  themeMode?: "auto" | "dark" | "light",
): void {
  const root = document.documentElement;
  const [r, g, b] = hexToRgb(bgColor);
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  const isDark =
    themeMode === "dark" ? true
    : themeMode === "light" ? false
    : luminance < 0.5;

  if (isDark) {
    // 暗色主题：表面色比背景逐步提亮
    root.style.setProperty("--surface-base", hexToRgba(bgColor, 0.72));
    root.style.setProperty("--surface-raised", lightenColor(bgColor, 0.08));
    root.style.setProperty("--surface-hover", lightenColor(bgColor, 0.15));
    root.style.setProperty("--surface-active", lightenColor(bgColor, 0.04));
    root.style.setProperty("--border", lightenColor(bgColor, 0.08));
    root.style.setProperty("--border-focus", blendColors(accentColor, bgColor, 0.3));
    root.style.setProperty("--text-primary", "#ebebeb");
    root.style.setProperty("--text-secondary", "#b0b0b0");
    root.style.setProperty("--text-tertiary", "#888888");
    root.style.setProperty("--text-quaternary", "#3a3a3a");
    // 面板边框
    root.style.setProperty("--panel-border", "rgba(255, 255, 255, 0.05)");
    // 组件令牌 — 暗色
    root.style.setProperty("--assistant-text", "#cccccc");
    root.style.setProperty("--input-focus-bg", lightenColor(bgColor, 0.12));
    root.style.setProperty("--scrollbar-thumb", "rgba(255, 255, 255, 0.12)");
    root.style.setProperty("--scrollbar-thumb-hover", "rgba(255, 255, 255, 0.24)");
    root.style.setProperty("--mode-switch-bg", lightenColor(bgColor, 0.04));
    root.style.setProperty("--mode-switch-active", lightenColor(bgColor, 0.15));
    root.style.setProperty("--tooltip-bg", blendColors(bgColor, "#000000", 0.85));
    root.style.setProperty("--toolbar-bg", blendColors(bgColor, "#1a1a28", 0.74));
    root.style.setProperty("--toolbar-border", "rgba(255, 255, 255, 0.06)");
    root.style.setProperty("--blockquote-border", blendColors(accentColor, bgColor, 0.3));
    root.style.setProperty("--welcome-card-bg", "rgba(255, 255, 255, 0.03)");
    root.style.setProperty("--welcome-card-border", "rgba(255, 255, 255, 0.06)");
    // AI 消息卡片 — 暗色
    root.style.setProperty("--assistant-bg", "rgba(255, 255, 255, 0.025)");
    root.style.setProperty("--assistant-border", "rgba(255, 255, 255, 0.05)");
    // 行内代码 — 暗色
    root.style.setProperty("--code-inline-text", "#c4a574");
    root.style.setProperty("--code-inline-bg", "rgba(255, 255, 255, 0.06)");
    // 代码块 — 暗色
    root.style.setProperty("--code-bg", darkenColor(bgColor, 0.03));
    root.style.setProperty("--code-header-bg", "rgba(255, 255, 255, 0.04)");
    // 引用块 — 暗色
    root.style.setProperty("--blockquote-bg", "rgba(129, 140, 248, 0.04)");
    // 气泡 — 暗色（降低对比度，融入桌面）
    root.style.setProperty("--bubble-bg", "rgba(255, 255, 255, 0.60)");
    root.style.setProperty("--bubble-bg-hover", "rgba(255, 255, 255, 0.75)");
    root.style.setProperty("--bubble-border", "rgba(255, 255, 255, 0.08)");
    root.style.setProperty("--bubble-shadow", "0 1px 3px rgba(0, 0, 0, 0.08)");
    root.style.setProperty("--bubble-shadow-hover", "0 2px 6px rgba(0, 0, 0, 0.14)");
    root.style.setProperty("--bubble-x-color", "#3b3b50");
    root.style.setProperty("--bubble-icon-idle", "#999999");
    root.style.setProperty("--reading-dot-shadow", "rgba(255, 255, 255, 0.9)");
    // 语法高亮 — 暗色
    root.style.setProperty("--syntax-keyword", "#c084fc");
    root.style.setProperty("--syntax-string", "#6ee7b7");
    root.style.setProperty("--syntax-number", "#fbbf24");
    root.style.setProperty("--syntax-type", "#67e8f9");
    root.style.setProperty("--syntax-function", "#a5b4fc");
    root.style.setProperty("--syntax-property", "#cbd5e1");
    root.style.setProperty("--syntax-operator", "#94a3b8");
  } else {
    // 亮色主题：表面色比背景逐步加深
    root.style.setProperty("--surface-base", hexToRgba(bgColor, 0.92));
    root.style.setProperty("--surface-raised", darkenColor(bgColor, 0.10));
    root.style.setProperty("--surface-hover", darkenColor(bgColor, 0.18));
    root.style.setProperty("--surface-active", darkenColor(bgColor, 0.05));
    root.style.setProperty("--border", darkenColor(bgColor, 0.14));
    root.style.setProperty("--border-focus", blendColors(accentColor, bgColor, 0.5));
    root.style.setProperty("--text-primary", "#1a1a1a");
    root.style.setProperty("--text-secondary", "#555555");
    root.style.setProperty("--text-tertiary", "#888888");
    root.style.setProperty("--text-quaternary", "#bbbbbb");
    // 面板边框
    root.style.setProperty("--panel-border", "rgba(0, 0, 0, 0.06)");
    // 组件令牌 — 亮色
    root.style.setProperty("--assistant-text", "#3a3a3a");
    root.style.setProperty("--input-focus-bg", darkenColor(bgColor, 0.12));
    root.style.setProperty("--scrollbar-thumb", "rgba(0, 0, 0, 0.18)");
    root.style.setProperty("--scrollbar-thumb-hover", "rgba(0, 0, 0, 0.32)");
    root.style.setProperty("--mode-switch-bg", darkenColor(bgColor, 0.06));
    root.style.setProperty("--mode-switch-active", darkenColor(bgColor, 0.16));
    root.style.setProperty("--tooltip-bg", blendColors(bgColor, "#ffffff", 0.92));
    root.style.setProperty("--toolbar-bg", blendColors(bgColor, "#ffffff", 0.82));
    root.style.setProperty("--toolbar-border", "rgba(0, 0, 0, 0.06)");
    root.style.setProperty("--blockquote-border", blendColors(accentColor, bgColor, 0.5));
    root.style.setProperty("--welcome-card-bg", "rgba(0, 0, 0, 0.02)");
    root.style.setProperty("--welcome-card-border", "rgba(0, 0, 0, 0.06)");
    // AI 消息卡片 — 亮色
    root.style.setProperty("--assistant-bg", "rgba(0, 0, 0, 0.02)");
    root.style.setProperty("--assistant-border", "rgba(0, 0, 0, 0.06)");
    // 行内代码 — 亮色
    root.style.setProperty("--code-inline-text", "#8b6914");
    root.style.setProperty("--code-inline-bg", "rgba(0, 0, 0, 0.05)");
    // 代码块 — 亮色
    root.style.setProperty("--code-bg", darkenColor(bgColor, 0.04));
    root.style.setProperty("--code-header-bg", "rgba(0, 0, 0, 0.04)");
    // 引用块 — 亮色
    root.style.setProperty("--blockquote-bg", "rgba(129, 140, 248, 0.06)");
    // 气泡 — 亮色（白色基底，柔和阴影）
    root.style.setProperty("--bubble-bg", "rgba(255, 255, 255, 0.85)");
    root.style.setProperty("--bubble-bg-hover", "rgba(255, 255, 255, 0.95)");
    root.style.setProperty("--bubble-border", "rgba(0, 0, 0, 0.12)");
    root.style.setProperty("--bubble-shadow", "0 1px 4px rgba(0, 0, 0, 0.08)");
    root.style.setProperty("--bubble-shadow-hover", "0 2px 8px rgba(0, 0, 0, 0.12)");
    root.style.setProperty("--bubble-x-color", "#777777");
    root.style.setProperty("--bubble-icon-idle", "#666666");
    root.style.setProperty("--reading-dot-shadow", "rgba(255, 255, 255, 0.8)");
    // 语法高亮 — 亮色
    root.style.setProperty("--syntax-keyword", "#7c3aed");
    root.style.setProperty("--syntax-string", "#059669");
    root.style.setProperty("--syntax-number", "#d97706");
    root.style.setProperty("--syntax-type", "#0891b2");
    root.style.setProperty("--syntax-function", "#4f46e5");
    root.style.setProperty("--syntax-property", "#475569");
    root.style.setProperty("--syntax-operator", "#64748b");
  }
}
