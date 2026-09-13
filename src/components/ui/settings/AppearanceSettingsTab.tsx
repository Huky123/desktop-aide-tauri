import { presetToConfig } from "../../../lib/presets";
import { THEME_PRESETS_DARK, THEME_PRESETS_LIGHT } from "../settingsOptions";
import type { AppConfig } from "../../../types";

export interface AppearanceSettingsTabProps {
  /** 设置面板的本地草稿配置（保存前的编辑副本） */
  localConfig: AppConfig;
  /** 合并写入草稿的某个字段 */
  patch: (partial: Partial<AppConfig>) => void;
}

/**
 * 设置面板「外观」页内容（主题预设 + 配色）。
 *
 * 不含外层动画容器——`motion.section` 由 SettingsPanel 持有。
 */
export function AppearanceSettingsTab({ localConfig, patch }: AppearanceSettingsTabProps) {
  return (
    <>
              {/* 深色主题 */}
              <p className="text-[10px] font-semibold uppercase tracking-wider" style={{ color: "var(--text-secondary)" }}>深色</p>
              <div className="grid grid-cols-3 gap-1.5">
                {THEME_PRESETS_DARK.map((preset) => {
                  const isSelected = localConfig.theme_preset === preset.id;
                  return (
                    <button
                      key={preset.id}
                      onClick={() => patch(presetToConfig(preset))}
                      className="theme-preset-button"
                      style={{
                        background: isSelected ? "var(--surface-hover)" : "var(--surface-active)",
                        border: `${isSelected ? 2 : 1}px solid`,
                        borderColor: isSelected ? preset.accent_color : "var(--border)",
                        color: "var(--text-secondary)",
                      }}
                    >
                      <span
                        className="w-4 h-4 rounded-full flex-shrink-0"
                        style={{
                          background: `linear-gradient(135deg, ${preset.accent_color} 50%, transparent 50%),
                            linear-gradient(135deg, transparent 50%, ${preset.bg_color} 50%)`,
                        }}
                      />
                      {preset.name}
                    </button>
                  );
                })}
              </div>

              {/* 浅色主题 */}
              <p className="text-[10px] font-semibold uppercase tracking-wider mt-2" style={{ color: "var(--text-secondary)" }}>浅色</p>
              <div className="grid grid-cols-3 gap-1.5">
                {THEME_PRESETS_LIGHT.map((preset) => {
                  const isSelected = localConfig.theme_preset === preset.id;
                  return (
                    <button
                      key={preset.id}
                      onClick={() => patch(presetToConfig(preset))}
                      className="theme-preset-button"
                      style={{
                        background: isSelected ? "var(--surface-hover)" : "var(--surface-active)",
                        border: `${isSelected ? 2 : 1}px solid`,
                        borderColor: isSelected ? preset.accent_color : "var(--border)",
                        color: "var(--text-secondary)",
                      }}
                    >
                      <span
                        className="w-4 h-4 rounded-full flex-shrink-0"
                        style={{
                          background: `linear-gradient(135deg, ${preset.accent_color} 50%, transparent 50%),
                            linear-gradient(135deg, transparent 50%, ${preset.bg_color} 50%)`,
                        }}
                      />
                      {preset.name}
                    </button>
                  );
                })}
              </div>

              {/* 自定义颜色 — 常驻显示，修改即切到 custom */}
              <p className="text-[10px] font-semibold uppercase tracking-wider mt-2" style={{ color: "var(--text-secondary)" }}>微调</p>
              <div className="settings-color-row">
                <div className="flex items-center gap-1.5">
                  <span className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>强调</span>
                  <input
                    type="color"
                    value={localConfig.accent_color}
                    onChange={(e) => patch({ accent_color: e.target.value, theme_preset: "custom" as AppConfig["theme_preset"] })}
                    className="w-6 h-6 rounded cursor-pointer border-none"
                    style={{ background: "none" }}
                  />
                </div>
                <div className="flex items-center gap-1.5">
                  <span className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>背景</span>
                  <input
                    type="color"
                    value={localConfig.bg_color}
                    onChange={(e) => patch({ bg_color: e.target.value, theme_preset: "custom" as AppConfig["theme_preset"] })}
                    className="w-6 h-6 rounded cursor-pointer border-none"
                    style={{ background: "none" }}
                  />
                </div>
                <span className="text-[10px] font-mono ml-auto" style={{ color: "var(--text-quaternary)" }}>
                  {localConfig.accent_color}
                </span>
              </div>

              {/* 不透明度 */}
              <div>
                <div className="flex items-center justify-between mb-1">
                  <span className="text-[11px]" style={{ color: "var(--text-secondary)" }}>背景板不透明度</span>
                  <span className="text-[11px] font-medium" style={{ color: "var(--text-tertiary)" }}>
                    {Math.round(localConfig.bg_opacity * 100)}%
                  </span>
                </div>
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={Math.round(localConfig.bg_opacity * 100)}
                  onChange={(e) =>
                    patch({
                      bg_opacity: Number(e.target.value) / 100,
                      theme_preset: "custom" as AppConfig["theme_preset"],
                    })
                  }
                  className="w-full h-1.5 rounded-full appearance-none cursor-pointer"
                  style={{
                    background: `linear-gradient(to right, ${localConfig.bg_color}, ${localConfig.accent_color})`,
                  }}
                />
              </div>
    </>
  );
}
