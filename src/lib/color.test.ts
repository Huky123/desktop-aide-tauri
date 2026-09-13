import { describe, it, expect, beforeEach } from "vitest";
import {
  hexToRgb,
  hexToRgba,
  lightenColor,
  darkenColor,
  blendColors,
  accentToBubble,
  applyThemeVars,
} from "./color";

describe("hexToRgb", () => {
  it("正确解析标准 6 位 hex", () => {
    expect(hexToRgb("#ff0000")).toEqual([255, 0, 0]);
    expect(hexToRgb("#00ff00")).toEqual([0, 255, 0]);
    expect(hexToRgb("#0000ff")).toEqual([0, 0, 255]);
  });

  it("正确解析白色和黑色", () => {
    expect(hexToRgb("#ffffff")).toEqual([255, 255, 255]);
    expect(hexToRgb("#000000")).toEqual([0, 0, 0]);
  });

  it("正确处理不带 # 前缀的 hex", () => {
    expect(hexToRgb("ff0000")).toEqual([255, 0, 0]);
    expect(hexToRgb("808080")).toEqual([128, 128, 128]);
  });

  it("正确解析小写字母 hex", () => {
    expect(hexToRgb("#ff00aa")).toEqual([255, 0, 170]);
    expect(hexToRgb("#aabbcc")).toEqual([170, 187, 204]);
  });

  it("正确解析默认主题色 #121216", () => {
    expect(hexToRgb("#121216")).toEqual([18, 18, 22]);
  });
});

describe("hexToRgba", () => {
  it("将 hex 转为带透明度的 rgba", () => {
    expect(hexToRgba("#ff0000", 1)).toBe("rgba(255, 0, 0, 1)");
    expect(hexToRgba("#ff0000", 0.5)).toBe("rgba(255, 0, 0, 0.5)");
    expect(hexToRgba("#ff0000", 0)).toBe("rgba(255, 0, 0, 0)");
  });

  it("支持不带 # 的 hex", () => {
    expect(hexToRgba("000000", 0.72)).toBe("rgba(0, 0, 0, 0.72)");
  });
});

describe("lightenColor", () => {
  it("amount=0 时返回原色", () => {
    expect(lightenColor("#ff0000", 0)).toBe("rgb(255, 0, 0)");
  });

  it("amount=1 时返回白色", () => {
    expect(lightenColor("#ff0000", 1)).toBe("rgb(255, 255, 255)");
    expect(lightenColor("#000000", 1)).toBe("rgb(255, 255, 255)");
  });

  it("amount=0.5 时正确计算中间色", () => {
    // (0 + (255-0)*0.5) = 127.5 → 128
    const result = lightenColor("#000000", 0.5);
    // All channels: 0 + 255*0.5 = 127.5 → 128
    expect(result).toBe("rgb(128, 128, 128)");
  });

  it("小数值提亮效果正确", () => {
    const result = lightenColor("#121216", 0.08);
    // r: 18 + (255-18)*0.08 = 18 + 18.96 = 36.96 → 37
    // g: 18 + (255-18)*0.08 = 37
    // b: 22 + (255-22)*0.08 = 22 + 18.64 = 40.64 → 41
    expect(result).toBe("rgb(37, 37, 41)");
  });
});

describe("darkenColor", () => {
  it("amount=0 时返回原色", () => {
    expect(darkenColor("#ffffff", 0)).toBe("rgb(255, 255, 255)");
  });

  it("amount=1 时返回黑色", () => {
    expect(darkenColor("#ffffff", 1)).toBe("rgb(0, 0, 0)");
    expect(darkenColor("#ff0000", 1)).toBe("rgb(0, 0, 0)");
  });

  it("amount=0.5 时正确计算中间色", () => {
    const result = darkenColor("#808080", 0.5);
    // 128 * 0.5 = 64
    expect(result).toBe("rgb(64, 64, 64)");
  });

  it("小数值加深效果正确", () => {
    const result = darkenColor("#ffffff", 0.06);
    // 255 * 0.94 = 239.7 → 240
    expect(result).toBe("rgb(240, 240, 240)");
  });
});

describe("blendColors", () => {
  it("ratio=1 时返回第一个颜色", () => {
    expect(blendColors("#ff0000", "#0000ff", 1)).toBe("rgb(255, 0, 0)");
  });

  it("ratio=0 时返回第二个颜色", () => {
    expect(blendColors("#ff0000", "#0000ff", 0)).toBe("rgb(0, 0, 255)");
  });

  it("ratio=0.5 时正确混合", () => {
    // r: 255*0.5 + 0*0.5 = 127.5 → 128
    // g: 0
    // b: 0*0.5 + 255*0.5 = 127.5 → 128
    expect(blendColors("#ff0000", "#0000ff", 0.5)).toBe("rgb(128, 0, 128)");
  });

  it("与背景混合强调色", () => {
    // accent=#818cf8 (129,140,248), bg=#121216 (18,18,22), ratio=0.3
    // r: 129*0.3 + 18*0.7 = 38.7 + 12.6 = 51.3 → 51
    // g: 140*0.3 + 18*0.7 = 42 + 12.6 = 54.6 → 55
    // b: 248*0.3 + 22*0.7 = 74.4 + 15.4 = 89.8 → 90
    expect(blendColors("#818cf8", "#121216", 0.3)).toBe("rgb(51, 55, 90)");
  });
});

describe("accentToBubble", () => {
  it("混合强调色和背景色生成气泡颜色", () => {
    // accent=#818cf8 (129,140,248), bg=#121216 (18,18,22), blend=0.12
    // r: 129*0.12 + 18*0.88 = 15.48 + 15.84 = 31.32 → 31
    // g: 140*0.12 + 18*0.88 = 16.8 + 15.84 = 32.64 → 33
    // b: 248*0.12 + 22*0.88 = 29.76 + 19.36 = 49.12 → 49
    expect(accentToBubble("#818cf8", 0.12, "#121216")).toBe("rgb(31, 33, 49)");
  });

  it("blend=0 时返回纯背景色", () => {
    expect(accentToBubble("#ff0000", 0, "#0000ff")).toBe("rgb(0, 0, 255)");
  });

  it("blend=1 时返回纯强调色", () => {
    expect(accentToBubble("#ff0000", 1, "#0000ff")).toBe("rgb(255, 0, 0)");
  });
});

describe("applyThemeVars", () => {
  beforeEach(() => {
    // 重置 CSS 变量
    const root = document.documentElement;
    root.removeAttribute("style");
  });

  it("暗色背景（低亮度）应用暗色主题", () => {
    // #121216 亮度 ≈ 0.072 < 0.3 → 暗色
    applyThemeVars("#121216", "#818cf8");

    const root = document.documentElement;
    expect(root.style.getPropertyValue("--surface-base")).toBe("rgba(18, 18, 22, 0.72)");
    expect(root.style.getPropertyValue("--text-primary")).toBe("#ebebeb");
    expect(root.style.getPropertyValue("--text-secondary")).toBe("#b0b0b0");
  });

  it("亮色背景（高亮度）应用亮色主题", () => {
    // #ffffff 亮度 = 1.0 > 0.3 → 亮色
    applyThemeVars("#ffffff", "#818cf8");

    const root = document.documentElement;
    // surface-base: rgba(255,255,255,0.92)
    expect(root.style.getPropertyValue("--surface-base")).toBe("rgba(255, 255, 255, 0.92)");
    expect(root.style.getPropertyValue("--text-primary")).toBe("#1a1a1a");
    expect(root.style.getPropertyValue("--text-secondary")).toBe("#555555");
  });

  it("亮度恰好 0.5 时应用亮色主题（边界条件）", () => {
    // luminance = 0.5, r=g=b: 0.299*r/255 + 0.587*r/255 + 0.114*r/255 = r/255
    // 当 r=127: 127/255 ≈ 0.498 < 0.5 → 暗色
    // 当 r=128: 128/255 ≈ 0.502 ≥ 0.5 → 亮色
    applyThemeVars("#808080", "#818cf8"); // luminance ≈ 0.502

    const root = document.documentElement;
    expect(root.style.getPropertyValue("--text-primary")).toBe("#1a1a1a");
  });

  it("亮度稍低于 0.5 时应用暗色主题", () => {
    applyThemeVars("#7f7f7f", "#818cf8"); // luminance ≈ 0.498

    const root = document.documentElement;
    expect(root.style.getPropertyValue("--text-primary")).toBe("#ebebeb");
  });

  it("暗色主题设置 border-focus 混合强调色", () => {
    applyThemeVars("#121216", "#818cf8");

    const root = document.documentElement;
    // blendColors(accent, bg, 0.3)
    const borderFocus = root.style.getPropertyValue("--border-focus");
    expect(borderFocus).toBe("rgb(51, 55, 90)");
  });

  it("亮色主题设置 border-focus 混合强调色（ratio=0.5）", () => {
    applyThemeVars("#ffffff", "#000000");
    // blendColors(#000000, #ffffff, 0.5) → 0*0.5 + 255*0.5 = 128
    const root = document.documentElement;
    expect(root.style.getPropertyValue("--border-focus")).toBe("rgb(128, 128, 128)");
  });

  it("设置所有文本颜色变量", () => {
    applyThemeVars("#121216", "#818cf8");

    const root = document.documentElement;
    expect(root.style.getPropertyValue("--text-primary")).toBeTruthy();
    expect(root.style.getPropertyValue("--text-secondary")).toBeTruthy();
    expect(root.style.getPropertyValue("--text-tertiary")).toBeTruthy();
    expect(root.style.getPropertyValue("--text-quaternary")).toBeTruthy();
  });

  it("设置所有表面颜色变量", () => {
    applyThemeVars("#ffffff", "#818cf8");

    const root = document.documentElement;
    expect(root.style.getPropertyValue("--surface-base")).toBeTruthy();
    expect(root.style.getPropertyValue("--surface-raised")).toBeTruthy();
    expect(root.style.getPropertyValue("--surface-hover")).toBeTruthy();
    expect(root.style.getPropertyValue("--surface-active")).toBeTruthy();
    expect(root.style.getPropertyValue("--border")).toBeTruthy();
  });
});
