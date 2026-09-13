import { beforeEach, describe, expect, it } from "vitest";
import { normalizeConfig, useConfigStore } from "./configStore";
import { DEFAULT_CONFIG } from "../types";
import type { AppConfig, RawConfigDto } from "../types";

/** 模拟"磁盘上存着非法/旧版取值"的场景：绕过类型约束喂给 normalize */
function rawConfig(fields: Record<string, unknown>): RawConfigDto {
  return fields as RawConfigDto;
}

/** store 只提供整体替换的 setConfig，这里封装出"部分更新"的调用方式 */
function patchConfig(partial: Partial<AppConfig>): void {
  const { config, setConfig } = useConfigStore.getState();
  setConfig({ ...config, ...partial });
}

describe("configStore", () => {
  beforeEach(() => {
    useConfigStore.setState({
      config: { ...DEFAULT_CONFIG },
    });
  });

  it("默认配置正确", () => {
    const { config } = useConfigStore.getState();
    // 默认 DeepSeek：面向国内用户，Key 获取门槛最低（与后端 default_provider 一致）
    expect(config.ai_provider).toBe("deepseek");
    expect(config.model).toBe("deepseek-chat");
    expect(config.bg_color).toBe("#121216");
    expect(config.bg_opacity).toBe(0.72);
    expect(config.accent_color).toBe("#818cf8");
  });

  it("setConfig 完整替换配置", () => {
    const newConfig = { ...useConfigStore.getState().config, bg_opacity: 0.5 };
    useConfigStore.getState().setConfig(newConfig);
    expect(useConfigStore.getState().config.bg_opacity).toBe(0.5);
  });

  it("默认主题模式为 auto", () => {
    expect(useConfigStore.getState().config.theme_mode).toBe("auto");
  });

  it("默认主题预设为 indigo", () => {
    expect(useConfigStore.getState().config.theme_preset).toBe("indigo");
  });

  it("可以更改主题模式", () => {
    patchConfig({ theme_mode: "dark" });
    expect(useConfigStore.getState().config.theme_mode).toBe("dark");
  });

  it("可以更改主题预设", () => {
    patchConfig({ theme_preset: "midnight" });
    expect(useConfigStore.getState().config.theme_preset).toBe("midnight");
    // 未修改的字段保持原值
    expect(useConfigStore.getState().config.accent_color).toBe("#818cf8");
  });
});

describe("normalizeConfig", () => {
  it("空对象返回默认配置", () => {
    const result = normalizeConfig({});
    expect(result.ai_provider).toBe(DEFAULT_CONFIG.ai_provider);
    expect(result.theme_mode).toBe("auto");
    expect(result.theme_preset).toBe("indigo");
  });

  it("补齐缺失的 theme_mode 和 theme_preset", () => {
    const result = normalizeConfig({ ai_provider: "openai" } as RawConfigDto);
    expect(result.ai_provider).toBe("openai");
    expect(result.theme_mode).toBe("auto");
    expect(result.theme_preset).toBe("indigo");
  });

  it("保留新增的 AI 服务商", () => {
    for (const ai_provider of ["gemini", "qwen", "kimi", "zhipu", "openrouter", "custom"] as const) {
      expect(normalizeConfig({ ai_provider })).toMatchObject({ ai_provider });
    }
  });

  it("非法 theme_mode 回退到默认值", () => {
    const result = normalizeConfig({ theme_mode: "invalid" } as unknown as RawConfigDto);
    expect(result.theme_mode).toBe("auto");
  });

  it("非法 theme_preset 回退到默认值", () => {
    const result = normalizeConfig({ theme_preset: "nonexistent" } as unknown as RawConfigDto);
    expect(result.theme_preset).toBe("indigo");
  });

  it("bg_opacity 越界 clamp", () => {
    expect(normalizeConfig({ bg_opacity: 5 }).bg_opacity).toBe(1);
    expect(normalizeConfig({ bg_opacity: -1 }).bg_opacity).toBe(0);
  });

  it("temperature 越界 clamp", () => {
    expect(normalizeConfig({ temperature: 999 }).temperature).toBe(2);
    expect(normalizeConfig({ temperature: -10 }).temperature).toBe(0);
  });

  it("NaN 数值回退到默认值", () => {
    const result = normalizeConfig({ bg_opacity: NaN, temperature: NaN, max_tokens: NaN } as RawConfigDto);
    expect(result.bg_opacity).toBe(DEFAULT_CONFIG.bg_opacity);
    expect(result.temperature).toBe(DEFAULT_CONFIG.temperature);
    expect(result.max_tokens).toBe(DEFAULT_CONFIG.max_tokens);
  });

  it("已知配置不做修改", () => {
    const input = { ...DEFAULT_CONFIG, theme_mode: "dark" as const, theme_preset: "midnight" as const };
    const result = normalizeConfig(input);
    expect(result.theme_mode).toBe("dark");
    expect(result.theme_preset).toBe("midnight");
  });

  it("新字段默认值 — bubble_auto_collapse/bubble_collapse_delay", () => {
    const result = normalizeConfig({});
    expect(result.bubble_auto_collapse).toBe(false);
    expect(result.bubble_collapse_delay).toBe(300);
  });

  it("非布尔 bubble_auto_collapse 回退到默认值", () => {
    const result = normalizeConfig({ bubble_auto_collapse: 1 } as unknown as RawConfigDto);
    expect(result.bubble_auto_collapse).toBe(false);
  });

  it("bubble_collapse_delay 越界 clamp", () => {
    expect(normalizeConfig({ bubble_collapse_delay: 1 }).bubble_collapse_delay).toBe(3);
    expect(normalizeConfig({ bubble_collapse_delay: 99999 }).bubble_collapse_delay).toBe(86400);
    expect(normalizeConfig({ bubble_collapse_delay: 7200 }).bubble_collapse_delay).toBe(7200);
  });

  // ── 识图 / 出图 两个独立维度 ──

  it("默认值保持最保守：OCR 识图 + 不是出图模型", () => {
    const config = normalizeConfig({});
    expect(config.vision_mode).toBe("off");
    expect(config.model_kind).toBe("chat");
    expect(DEFAULT_CONFIG.model_kind).toBe("chat");
  });

  it("识图与出图可以同时开启（不再互斥）", () => {
    const config = normalizeConfig({ vision_mode: "on", model_kind: "image" });
    expect(config.vision_mode).toBe("on");
    expect(config.model_kind).toBe("image");
  });

  it("旧版 model_kind=vision 迁移为 vision_mode=on（出图维度回落具体值）", () => {
    const config = normalizeConfig(rawConfig({ vision_mode: "off", model_kind: "vision" }));
    expect(config.vision_mode).toBe("on");
    expect(config.model_kind).toBe("chat");
  });

  it("遗留的 auto 解析成具体取值，避免两个选项都没选中", () => {
    expect(normalizeConfig(rawConfig({ model_kind: "auto" })).model_kind).toBe("chat");
    expect(normalizeConfig(rawConfig({ vision_mode: "auto" })).vision_mode).toBe("off");
  });

  it("现行取值不被转换逻辑误改", () => {
    expect(normalizeConfig({ model_kind: "image" }).model_kind).toBe("image");
    expect(normalizeConfig({ model_kind: "chat" }).model_kind).toBe("chat");
    expect(normalizeConfig({ vision_mode: "on" }).vision_mode).toBe("on");
  });

  it("非法取值回落默认值", () => {
    expect(normalizeConfig(rawConfig({ model_kind: "nonsense" })).model_kind).toBe("chat");
    expect(normalizeConfig(rawConfig({ vision_mode: "nonsense" })).vision_mode).toBe("off");
  });
});
