import { create } from "zustand";
import type { AppConfig, RawConfigDto } from "../types";
import { DEFAULT_CONFIG } from "../types";

const VALID_THEME_MODES = ["auto", "dark", "light"] as const;
const VALID_THEME_PRESETS = [
  "indigo", "midnight", "emerald", "cloud", "sakura", "moss", "custom",
] as const;
const VALID_PROVIDERS = [
  "anthropic",
  "openai",
  "xai",
  "gemini",
  "deepseek",
  "qwen",
  "kimi",
  "zhipu",
  "openrouter",
  "ollama",
  "custom",
] as const;

/** 校验并合并后端返回的原始配置，补齐缺失字段、修正非法值 */
export function normalizeConfig(raw: RawConfigDto): AppConfig {
  const config = { ...DEFAULT_CONFIG, ...raw };

  // 校验 ai_provider
  if (!(VALID_PROVIDERS as readonly string[]).includes(config.ai_provider)) {
    config.ai_provider = DEFAULT_CONFIG.ai_provider;
  }

  // 校验 theme_mode
  if (!(VALID_THEME_MODES as readonly string[]).includes(config.theme_mode)) {
    config.theme_mode = DEFAULT_CONFIG.theme_mode;
  }

  // 校验 theme_preset
  if (!(VALID_THEME_PRESETS as readonly string[]).includes(config.theme_preset)) {
    config.theme_preset = DEFAULT_CONFIG.theme_preset;
  }

  // 数值边界 clamp
  config.bg_opacity = clamp(config.bg_opacity, 0, 1, DEFAULT_CONFIG.bg_opacity);
  config.temperature = clamp(config.temperature, 0, 2, DEFAULT_CONFIG.temperature);
  config.max_tokens = clampInt(config.max_tokens, 1, 200000, DEFAULT_CONFIG.max_tokens);
  config.panel_width = clampInt(config.panel_width, 420, 4000, DEFAULT_CONFIG.panel_width);
  config.panel_height = clampInt(config.panel_height, 600, 4000, DEFAULT_CONFIG.panel_height);

  // 校验布尔字段
  if (typeof config.bubble_auto_collapse !== "boolean") {
    config.bubble_auto_collapse = DEFAULT_CONFIG.bubble_auto_collapse;
  }
  if (typeof config.enable_tools !== "boolean") {
    config.enable_tools = DEFAULT_CONFIG.enable_tools;
  }
  if (typeof config.enable_web_search !== "boolean") {
    config.enable_web_search = DEFAULT_CONFIG.enable_web_search;
  }

  // 图片输入方式 / 模型用途在设置界面都是二选一：`auto` 只是旧配置的遗留值，
  // 后端加载配置时会按模型解析成具体取值，这里兜底转换，保证控件不会两个都没选中。
  if ((config.vision_mode as string) === "auto") {
    config.vision_mode = "off";
  }
  if (!["off", "on"].includes(config.vision_mode)) {
    config.vision_mode = DEFAULT_CONFIG.vision_mode;
  }

  // 旧版四选一的「识图」：识图维度独立出去，出图维度回落具体取值
  if ((config.model_kind as string) === "vision") {
    config.vision_mode = "on";
    config.model_kind = "chat";
  }
  if ((config.model_kind as string) === "auto") {
    config.model_kind = "chat";
  }
  if (!["chat", "image"].includes(config.model_kind)) {
    config.model_kind = DEFAULT_CONFIG.model_kind;
  }

  // bubble_collapse_delay 范围 3 秒~24 小时
  config.bubble_collapse_delay = clampInt(
    config.bubble_collapse_delay,
    3,
    86400,
    DEFAULT_CONFIG.bubble_collapse_delay,
  );

  return config;
}

function clamp(val: unknown, min: number, max: number, fallback: number): number {
  if (typeof val !== "number" || Number.isNaN(val)) return fallback;
  return Math.max(min, Math.min(max, val));
}

function clampInt(val: unknown, min: number, max: number, fallback: number): number {
  const n = clamp(val, min, max, fallback);
  return Math.round(n);
}

interface ConfigState {
  config: AppConfig;
  setConfig: (config: AppConfig) => void;
}

export const useConfigStore = create<ConfigState>((set) => ({
  config: { ...DEFAULT_CONFIG },

  setConfig: (config) => set({ config }),
}));
