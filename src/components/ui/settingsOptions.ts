import { THEME_PRESETS } from "../../lib/presets";
import type { AppConfig } from "../../types";

export const PROVIDERS: {
  value: AppConfig["ai_provider"];
  label: string;
  brandColor: string;
}[] = [
  { value: "anthropic", label: "Anthropic", brandColor: "#d4a574" },
  { value: "openai", label: "OpenAI", brandColor: "#10a37f" },
  { value: "xai", label: "xAI (Grok)", brandColor: "#111111" },
  { value: "gemini", label: "Google Gemini", brandColor: "#4285f4" },
  { value: "deepseek", label: "DeepSeek", brandColor: "#4d6bfe" },
  { value: "qwen", label: "通义千问", brandColor: "#615ced" },
  { value: "kimi", label: "Kimi", brandColor: "#111827" },
  { value: "zhipu", label: "智谱 AI", brandColor: "#2563eb" },
  { value: "openrouter", label: "OpenRouter", brandColor: "#7c3aed" },
  { value: "ollama", label: "Ollama", brandColor: "#fafafa" },
  { value: "custom", label: "自定义兼容服务", brandColor: "#94a3b8" },
];

export const THEME_PRESETS_DARK = THEME_PRESETS.filter((preset) => {
  const hex = preset.bg_color.replace("#", "");
  const brightness = parseInt(hex.slice(0, 2), 16)
    + parseInt(hex.slice(2, 4), 16)
    + parseInt(hex.slice(4, 6), 16);
  return brightness < 300;
});

export const THEME_PRESETS_LIGHT = THEME_PRESETS.filter((preset) => {
  const hex = preset.bg_color.replace("#", "");
  const brightness = parseInt(hex.slice(0, 2), 16)
    + parseInt(hex.slice(2, 4), 16)
    + parseInt(hex.slice(4, 6), 16);
  return brightness >= 300;
});

export const PROVIDER_DEFAULTS: Record<AppConfig["ai_provider"], Partial<AppConfig>> = {
  anthropic: { model: "claude-sonnet-4-5-20250929", api_base: "" },
  openai: { model: "gpt-4o", api_base: "" },
  xai: { model: "grok-2-image-1212", api_base: "" },
  gemini: { model: "gemini-2.5-flash", api_base: "" },
  deepseek: { model: "deepseek-chat", api_base: "" },
  qwen: { model: "qwen-plus", api_base: "" },
  kimi: { model: "moonshot-v1-8k", api_base: "" },
  zhipu: { model: "glm-4.5-air", api_base: "" },
  openrouter: { model: "openai/gpt-4o-mini", api_base: "" },
  ollama: { model: "llama3.2", ollama_endpoint: "http://localhost:11434", ollama_model: "llama3.2", api_base: "" },
  custom: { model: "", api_base: "" },
};

export type SettingsTab = "ai" | "appearance" | "general";

export const SETTINGS_TABS: { key: SettingsTab; label: string }[] = [
  { key: "ai", label: "AI 服务" },
  { key: "appearance", label: "外观" },
  { key: "general", label: "通用" },
];

export const BUBBLE_COLLAPSE_OPTIONS = [
  { label: "永不", delay: null },
  { label: "30 秒", delay: 30 },
  { label: "1 分钟", delay: 60 },
  { label: "5 分钟", delay: 300 },
  { label: "15 分钟", delay: 900 },
  { label: "自定义", delay: "custom" },
] as const;

export type DelayUnit = "seconds" | "minutes" | "hours";

export const DELAY_UNIT_SECONDS: Record<DelayUnit, number> = {
  seconds: 1,
  minutes: 60,
  hours: 3600,
};

export const FIXED_COLLAPSE_DELAYS = new Set([30, 60, 300, 900]);

export function delayToCustomValue(delay: number): { value: string; unit: DelayUnit } {
  if (delay % 3600 === 0) return { value: String(delay / 3600), unit: "hours" };
  if (delay % 60 === 0) return { value: String(delay / 60), unit: "minutes" };
  return { value: String(delay), unit: "seconds" };
}
