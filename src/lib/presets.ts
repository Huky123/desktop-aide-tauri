import type { AppConfig } from "../types";

interface ThemePreset {
  id: AppConfig["theme_preset"] & string;
  name: string;
  accent_color: string;
  bg_color: string;
  bg_opacity: number;
}

export const THEME_PRESETS: ThemePreset[] = [
  // ── 深色系列 ──
  {
    id: "indigo",
    name: "靛蓝",
    accent_color: "#818cf8",
    bg_color: "#121216",
    bg_opacity: 0.72,
  },
  {
    id: "midnight",
    name: "午夜",
    accent_color: "#ff8c42",
    bg_color: "#121212",
    bg_opacity: 0.78,
  },
  {
    id: "emerald",
    name: "翠绿",
    accent_color: "#34d399",
    bg_color: "#121612",
    bg_opacity: 0.72,
  },
  // ── 浅色系列 ──
  {
    id: "cloud",
    name: "云朵",
    accent_color: "#3b82f6",
    bg_color: "#eff0f2",
    bg_opacity: 0.88,
  },
  {
    id: "sakura",
    name: "樱花",
    accent_color: "#ec4899",
    bg_color: "#faf7f8",
    bg_opacity: 0.88,
  },
  {
    id: "moss",
    name: "苔藓",
    accent_color: "#84a98c",
    bg_color: "#e8ede4",
    bg_opacity: 0.88,
  },
];

/** 将预设应用到 config 的部分字段 */
export function presetToConfig(preset: ThemePreset): Partial<AppConfig> {
  return {
    theme_preset: preset.id as AppConfig["theme_preset"],
    theme_mode: "auto",
    accent_color: preset.accent_color,
    bg_color: preset.bg_color,
    bg_opacity: preset.bg_opacity,
    msg_user_bg: "",
    msg_user_border: "",
  };
}

