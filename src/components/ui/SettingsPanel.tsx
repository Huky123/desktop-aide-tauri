import { useState, useCallback, useMemo, useRef, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { useConfigStore } from "../../store/configStore";
import { useSessionStore } from "../../store/sessionStore";
import { tauriApi } from "../../services/tauriApi";
import { FormField } from "./FormField";
import { AiServiceTab } from "./settings/AiServiceTab";
import { AppearanceSettingsTab } from "./settings/AppearanceSettingsTab";
import type { AppConfig, StorageInfo } from "../../types";
import {
  BUBBLE_COLLAPSE_OPTIONS,
  DELAY_UNIT_SECONDS,
  FIXED_COLLAPSE_DELAYS,
  PROVIDER_DEFAULTS,
  SETTINGS_TABS,
  delayToCustomValue,
  type DelayUnit,
  type SettingsTab,
} from "./settingsOptions";

interface SettingsPanelProps {
  onClose: () => void;
}

export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const config = useConfigStore((s) => s.config);
  const setConfig = useConfigStore((s) => s.setConfig);
  const [localConfig, setLocalConfig] = useState<AppConfig>({ ...config });
  const initialCustomDelay = useMemo(
    () => FIXED_COLLAPSE_DELAYS.has(config.bubble_collapse_delay)
      ? { value: "1", unit: "hours" as DelayUnit }
      : delayToCustomValue(config.bubble_collapse_delay),
    [config.bubble_collapse_delay],
  );
  const [customDelayValue, setCustomDelayValue] = useState(initialCustomDelay.value);
  const [customDelayUnit, setCustomDelayUnit] = useState<DelayUnit>(initialCustomDelay.unit);
  const providerModelCache = useRef<Record<string, string>>({});
  const providerApiKeyCache = useRef<Record<string, string>>({});
  const providerApiBaseCache = useRef<Record<string, string>>({});

  useEffect(() => {
    providerModelCache.current[config.ai_provider] = config.model;
    providerApiKeyCache.current[config.ai_provider] = config.api_key;
    providerApiBaseCache.current[config.ai_provider] = config.api_base;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // 仅挂载时从持久化配置初始化，不需要追踪 config 变化

  const [saving, setSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<SettingsTab>("ai");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [showCloseConfirm, setShowCloseConfirm] = useState(false);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [clearing, setClearing] = useState(false);

  // ── 数据存储位置 ──
  const [storageInfo, setStorageInfo] = useState<StorageInfo | null>(null);
  const [pendingMoveTarget, setPendingMoveTarget] = useState<string | null>(null);
  const [moving, setMoving] = useState(false);

  useEffect(() => {
    tauriApi.getStorageInfo().then(setStorageInfo).catch(console.error);
  }, []);

  const handlePickStorageDir = async () => {
    const dir = await open({ directory: true, title: "选择数据存储位置" });
    if (typeof dir === "string" && dir) {
      setPendingMoveTarget(dir);
    }
  };

  const handleMoveData = async (target: string) => {
    if (moving) return;
    setMoving(true);
    toast.info("正在迁移数据，应用将自动重启…", { duration: 4000 });
    try {
      // 成功后后端会立即重启应用；此调用通常不会正常返回
      await tauriApi.moveDataDir(target);
      setMoving(false);
      toast.success("数据已迁移，正等待重启");
    } catch (err) {
      setMoving(false);
      toast.error(`迁移失败: ${err}`);
    }
  };

  const displayDataDir = storageInfo?.data_dir ?? "";
  const isCustomDir = storageInfo?.is_custom ?? false;
  const hasChanges = useMemo(
    () => JSON.stringify(localConfig) !== JSON.stringify(config),
    [localConfig, config],
  );
  const aiValidationError = useMemo(() => {
    if (!localConfig.model.trim()) return "请填写模型名称";
    if (localConfig.ai_provider === "custom" && !localConfig.api_base.trim()) {
      return "自定义服务需要填写兼容 API 地址";
    }
    return "";
  }, [localConfig.ai_provider, localConfig.api_base, localConfig.model]);

  const customDelaySelected = localConfig.bubble_auto_collapse
    && !FIXED_COLLAPSE_DELAYS.has(localConfig.bubble_collapse_delay);
  const customDelaySeconds = Number(customDelayValue) * DELAY_UNIT_SECONDS[customDelayUnit];
  const bubbleValidationError = customDelaySelected
    && (!Number.isFinite(customDelaySeconds) || customDelaySeconds < 3 || customDelaySeconds > 86400)
    ? "自定义时间需在 3 秒到 24 小时之间"
    : "";

  const patch = useCallback(
    (partial: Partial<AppConfig>) => setLocalConfig((prev) => ({ ...prev, ...partial })),
    [],
  );

  const updateCustomDelay = useCallback((value: string, unit: DelayUnit) => {
    setCustomDelayValue(value);
    setCustomDelayUnit(unit);
    const seconds = Number(value) * DELAY_UNIT_SECONDS[unit];
    if (Number.isFinite(seconds) && seconds >= 3 && seconds <= 86400) {
      patch({
        bubble_auto_collapse: true,
        bubble_collapse_delay: Math.round(seconds),
      });
    }
  }, [patch]);

  /** 记住当前服务商已填写的字段（切换服务商时回填）；供 AiServiceTab 调用 */
  const rememberProviderField = useCallback(
    (field: "model" | "api_key" | "api_base", value: string) => {
      const cache =
        field === "model"
          ? providerModelCache
          : field === "api_key"
            ? providerApiKeyCache
            : providerApiBaseCache;
      cache.current[localConfig.ai_provider] = value;
    },
    [localConfig.ai_provider],
  );

  const handleProviderChange = (provider: AppConfig["ai_provider"]) => {
    // 离开当前提供商前，缓存模型和密钥
    providerModelCache.current[localConfig.ai_provider] = localConfig.model;
    providerApiKeyCache.current[localConfig.ai_provider] = localConfig.api_key;
    providerApiBaseCache.current[localConfig.ai_provider] = localConfig.api_base;
    const defaults = PROVIDER_DEFAULTS[provider];
    patch({
      ai_provider: provider,
      api_key: providerApiKeyCache.current[provider] || "",
      model: providerModelCache.current[provider] || defaults.model || "",
      api_base: providerApiBaseCache.current[provider] ?? defaults.api_base ?? "",
      ...(provider === "ollama" ? { ollama_endpoint: defaults.ollama_endpoint } : {}),
    });
  };

  const handleClose = useCallback(() => {
    if (hasChanges && !showCloseConfirm) {
      setShowCloseConfirm(true);
    } else {
      onClose();
    }
  }, [hasChanges, onClose, showCloseConfirm]);

  const confirmDiscard = useCallback(() => {
    onClose();
  }, [onClose]);

  const cancelDiscard = useCallback(() => {
    setShowCloseConfirm(false);
  }, []);

  const handleSave = async () => {
    if (!hasChanges || saving || aiValidationError || bubbleValidationError) return;
    setSaving(true);
    try {
      await tauriApi.saveConfig(localConfig);
      setConfig(localConfig);
      toast.success("配置已保存");
    } catch (err) {
      toast.error(`保存失败: ${err}`);
    } finally {
      setSaving(false);
    }
  };

  const handleClearData = async () => {
    if (clearing) return;
    setClearing(true);
    try {
      await tauriApi.clearAllData();
      useSessionStore.getState().clearMessages();
      useSessionStore.getState().setConversations([]);
      useSessionStore.getState().setCurrentConversationId("default");
      setShowClearConfirm(false);
      toast.success("所有对话数据已清除");
    } catch (err) {
      toast.error(`清除失败: ${err}`);
    } finally {
      setClearing(false);
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* 头部 */}
      {showCloseConfirm ? (
        <div data-tauri-drag-region className="settings-header" style={{ background: "var(--surface-sunken)" }}>
          <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
            放弃未保存的更改？
          </span>
          <div className="flex items-center gap-1.5">
            <button
              onClick={cancelDiscard}
              className="px-2.5 py-1 rounded text-[11px] font-medium transition-colors duration-150"
              style={{ color: "var(--text-secondary)", background: "var(--surface-hover)" }}
            >
              继续编辑
            </button>
            <button
              onClick={confirmDiscard}
              className="px-2.5 py-1 rounded text-[11px] font-semibold transition-colors duration-150"
              style={{ color: "#fff", background: "#ef4444" }}
            >
              放弃
            </button>
          </div>
        </div>
      ) : (
        <div data-tauri-drag-region className="settings-header">
          <div className="cursor-grab active:cursor-grabbing">
            <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>设置</h3>
            <p className="text-[10px] mt-0.5" style={{ color: "var(--text-tertiary)" }}>
              {hasChanges ? "有未保存的更改" : "配置已同步"}
            </p>
          </div>
          <button onClick={handleClose} className="btn-icon" aria-label="关闭设置">
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      )}

      {/* Tab 导航 */}
      <div className="flex relative mx-3 mt-2" role="tablist">
        {SETTINGS_TABS.map((tab) => {
          const isActive = activeTab === tab.key;
          return (
            <button
              key={tab.key}
              role="tab"
              aria-selected={isActive}
              onClick={() => setActiveTab(tab.key)}
              className="flex-1 py-2 text-xs font-semibold relative transition-colors duration-150"
              style={{
                color: isActive ? "var(--text-primary)" : "var(--text-tertiary)",
              }}
            >
              {tab.label}
              {isActive && (
                <motion.div
                  layoutId="settings-tab-underline"
                  className="absolute bottom-0 left-2 right-2 h-0.5 rounded-full"
                  style={{ background: "var(--accent)" }}
                  transition={{ type: "spring", stiffness: 500, damping: 34 }}
                />
              )}
            </button>
          );
        })}
      </div>

      {/* Tab 内容 */}
      <div className="flex-1 overflow-y-auto px-3 py-1">
        <AnimatePresence mode="wait">
          {activeTab === "ai" ? (
            <motion.section
              key="ai"
              initial={{ opacity: 0, x: 12 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -12 }}
              transition={{ duration: 0.18, ease: "easeOut" }}
              className="space-y-3 mt-3"
            >
              <AiServiceTab
                localConfig={localConfig}
                patch={patch}
                rememberProviderField={rememberProviderField}
                showAdvanced={showAdvanced}
                setShowAdvanced={setShowAdvanced}
                aiValidationError={aiValidationError}
                handleProviderChange={handleProviderChange}
              />
            </motion.section>
          ) : activeTab === "general" ? (
            <motion.section
              key="general"
              initial={{ opacity: 0, x: 12 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 12 }}
              transition={{ duration: 0.18, ease: "easeOut" }}
              className="space-y-3 mt-3"
            >
              {/* ── AI 能力（工具调用 + 联网搜索）── */}
              <div
                className="settings-section-card"
                style={{
                  background: "var(--surface-active)",
                  border: "1px solid var(--bubble-border-soft)",
                }}
              >
                <div className="settings-section-card-header">
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M13 2L3 14h7l-1 8 10-12h-7l1-8z" />
                  </svg>
                  <span className="settings-section-card-title">AI 能力</span>
                </div>

                {/* 工具调用 */}
                <div className="settings-option-row settings-option-column">
                  <div>
                    <span className="settings-option-label-text">启用工具调用</span>
                    <p className="settings-option-desc">
                      AI 可调用工具完成任务：获取当前时间、创建 / 查看 / 取消定时提醒。
                    </p>
                  </div>
                  <div className="settings-toggle-row">
                    <button
                      type="button"
                      role="switch"
                      aria-checked={localConfig.enable_tools}
                      aria-label="启用工具调用"
                      className="settings-toggle"
                      onClick={() => patch({ enable_tools: !localConfig.enable_tools })}
                      style={{
                        background: localConfig.enable_tools
                          ? "var(--accent)"
                          : "var(--mode-switch-bg)",
                      }}
                    >
                      <span
                        className="settings-toggle-knob"
                        style={{
                          transform: localConfig.enable_tools
                            ? "translateX(18px)"
                            : "translateX(0)",
                          transition: "transform 0.2s ease",
                        }}
                      />
                    </button>
                  </div>
                </div>

                {/* 联网搜索 */}
                <div
                  className="settings-option-row settings-option-column"
                  style={{
                    borderTop: "1px solid var(--bubble-border-soft)",
                    paddingTop: 10,
                  }}
                >
                  <div>
                    <span className="settings-option-label-text">允许 AI 联网搜索</span>
                    <p className="settings-option-desc">
                      需要最新信息（新闻、行情、赛事等）时自动搜索网页并附来源链接。
                    </p>
                  </div>
                  <div className="settings-toggle-row">
                    <button
                      type="button"
                      role="switch"
                      aria-checked={localConfig.enable_web_search}
                      aria-label="允许 AI 联网搜索"
                      className="settings-toggle"
                      onClick={() => patch({ enable_web_search: !localConfig.enable_web_search })}
                      style={{
                        background: localConfig.enable_web_search
                          ? "var(--accent)"
                          : "var(--mode-switch-bg)",
                      }}
                    >
                      <span
                        className="settings-toggle-knob"
                        style={{
                          transform: localConfig.enable_web_search
                            ? "translateX(18px)"
                            : "translateX(0)",
                          transition: "transform 0.2s ease",
                        }}
                      />
                    </button>
                  </div>
                </div>

                {localConfig.enable_web_search && (
                  <div
                    className="settings-option-row settings-option-column"
                    style={{ paddingTop: 2 }}
                  >
                    <FormField
                      label="Tavily API Key（可选）"
                      type="password"
                      value={localConfig.tavily_api_key}
                      onChange={(v) => patch({ tavily_api_key: v })}
                      placeholder="tvly-..."
                      hint="在 tavily.com 免费注册，结果更稳定；留空时自动使用必应 / 百度等免 key 搜索源"
                    />
                  </div>
                )}
              </div>

              {/* ── 气泡行为 ── */}
              <div
                className="settings-section-card"
                style={{
                  background: "var(--surface-active)",
                  border: "1px solid var(--bubble-border-soft)",
                }}
              >
                {/* 卡片标题 */}
                <div className="settings-section-card-header">
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                  </svg>
                  <span className="settings-section-card-title">气泡行为</span>
                </div>

                <div className="settings-option-row settings-option-column">
                  <div>
                    <span className="settings-option-label-text">关闭面板后收折</span>
                    <p className="settings-option-desc">到达所选时间后，气泡收为屏幕边缘细条</p>
                  </div>
                  <div className="settings-choice-grid" role="radiogroup" aria-label="气泡收折时间">
                    {BUBBLE_COLLAPSE_OPTIONS.map((option) => {
                      const selected = option.delay === null
                        ? !localConfig.bubble_auto_collapse
                        : option.delay === "custom"
                          ? customDelaySelected
                          : localConfig.bubble_auto_collapse
                            && localConfig.bubble_collapse_delay === option.delay;
                      return (
                        <button
                          key={option.label}
                          type="button"
                          role="radio"
                          aria-checked={selected}
                          className={`settings-choice${selected ? " selected" : ""}`}
                          onClick={() => {
                            if (option.delay === "custom") {
                              const seconds = Number(customDelayValue) * DELAY_UNIT_SECONDS[customDelayUnit];
                              patch({
                                bubble_auto_collapse: true,
                                bubble_collapse_delay:
                                  Number.isFinite(seconds) && seconds >= 3 && seconds <= 86400
                                    ? Math.round(seconds)
                                    : 3600,
                              });
                              return;
                            }
                            patch({
                              bubble_auto_collapse: option.delay !== null,
                              ...(option.delay !== null ? { bubble_collapse_delay: option.delay } : {}),
                            });
                          }}
                        >
                          {option.label}
                        </button>
                      );
                    })}
                  </div>
                  {customDelaySelected && (
                    <div className="settings-custom-delay">
                      <input
                        type="number"
                        inputMode="decimal"
                        min={customDelayUnit === "seconds" ? 3 : customDelayUnit === "minutes" ? 0.05 : 1 / 1200}
                        max={customDelayUnit === "seconds" ? 86400 : customDelayUnit === "minutes" ? 1440 : 24}
                        step={customDelayUnit === "seconds" ? 1 : 0.5}
                        value={customDelayValue}
                        onChange={(event) => updateCustomDelay(event.target.value, customDelayUnit)}
                        className="settings-number-input"
                        aria-label="自定义收折时间"
                      />
                      <select
                        value={customDelayUnit}
                        onChange={(event) => {
                          const unit = event.target.value as DelayUnit;
                          const currentSeconds = localConfig.bubble_collapse_delay;
                          const value = String(currentSeconds / DELAY_UNIT_SECONDS[unit]);
                          updateCustomDelay(value, unit);
                        }}
                        aria-label="自定义收折时间单位"
                      >
                        <option value="seconds">秒</option>
                        <option value="minutes">分钟</option>
                        <option value="hours">小时</option>
                      </select>
                      <span>3 秒 - 24 小时</span>
                    </div>
                  )}
                  {bubbleValidationError && (
                    <p className="settings-validation" role="alert">{bubbleValidationError}</p>
                  )}
                </div>
              </div>

              {/* ── 数据存储位置 ── */}
              <div
                className="settings-section-card"
                style={{
                  background: "var(--surface-active)",
                  border: "1px solid var(--bubble-border-soft)",
                }}
              >
                {/* 卡片标题 */}
                <div className="settings-section-card-header">
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                  </svg>
                  <span className="settings-section-card-title">存储位置</span>
                </div>

                <p className="settings-option-desc" style={{ padding: "0 0 8px 0", margin: 0 }}>
                  配置、聊天记录与图片保存在所选目录，迁移后应用会自动重启
                </p>

                <div
                  className="settings-storage-path"
                  title={displayDataDir}
                  style={{ color: "var(--text-secondary)" }}
                >
                  {displayDataDir || "正在获取存储位置…"}
                </div>

                {pendingMoveTarget ? (
                  <div className="rounded-lg p-3 space-y-2.5 mt-2" style={{ background: "rgba(129, 140, 248, 0.07)", border: "1px solid rgba(129, 140, 248, 0.18)" }}>
                    <p className="text-[11px] leading-relaxed" style={{ color: "var(--text-primary)" }}>
                      将把全部数据迁移到：<br />
                      <span className="text-[10px] font-mono" style={{ color: "var(--text-secondary)", wordBreak: "break-all" }}>
                        {pendingMoveTarget}
                      </span>
                    </p>
                    <p className="text-[10px] leading-relaxed" style={{ color: "var(--text-tertiary)" }}>
                      迁移为复制方式，原目录数据保留。确认后应用将自动重启。
                    </p>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => setPendingMoveTarget(null)}
                        disabled={moving}
                        className="px-3 py-1.5 rounded-md text-[11px] font-medium transition-colors duration-150"
                        style={{ background: "var(--surface-raised)", color: "var(--text-secondary)" }}
                      >
                        取消
                      </button>
                      <button
                        onClick={() => void handleMoveData(pendingMoveTarget)}
                        disabled={moving}
                        className="px-3 py-1.5 rounded-md text-[11px] font-semibold transition-all duration-150 flex items-center gap-1.5"
                        style={{ background: "var(--accent)", color: "#fff", opacity: moving ? 0.6 : 1 }}
                      >
                        {moving ? (
                          <>
                            <svg className="w-3 h-3 animate-spin" fill="none" viewBox="0 0 24 24">
                              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                            </svg>
                            迁移中…
                          </>
                        ) : (
                          "开始迁移"
                        )}
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="flex items-center gap-2 mt-2">
                    <button
                      onClick={() => void handlePickStorageDir()}
                      disabled={moving}
                      className="px-3 py-1.5 rounded-md text-[11px] font-medium transition-colors duration-150"
                      style={{ background: "var(--surface-raised)", color: "var(--text-primary)", border: "1px solid var(--border)" }}
                    >
                      更改位置…
                    </button>
                    {isCustomDir && (
                      <button
                        onClick={() => setPendingMoveTarget(storageInfo!.default_dir)}
                        disabled={moving}
                        className="px-3 py-1.5 rounded-md text-[11px] font-medium transition-colors duration-150"
                        style={{ background: "var(--surface-raised)", color: "var(--text-secondary)" }}
                      >
                        恢复默认位置
                      </button>
                    )}
                  </div>
                )}
              </div>

              {/* ── 数据管理 ── */}
              <div
                className="settings-section-card"
                style={{
                  background: "var(--surface-active)",
                  border: "1px solid rgba(239, 68, 68, 0.12)",
                }}
              >
                {/* 卡片标题 */}
                <div className="settings-section-card-header">
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2} style={{ color: "#ef4444" }}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <span className="settings-section-card-title" style={{ color: "#f87171" }}>数据管理</span>
                </div>

                <p className="settings-option-desc" style={{ padding: "0 0 10px 0", margin: 0 }}>
                  清除所有对话记录、消息和 AI 上下文，此操作不可撤销
                </p>

                {showClearConfirm ? (
                  <div
                    className="rounded-lg p-3 space-y-2.5"
                    style={{
                      background: "rgba(239, 68, 68, 0.06)",
                      border: "1px solid rgba(239, 68, 68, 0.18)",
                    }}
                  >
                    <div className="flex items-start gap-2">
                      <svg className="w-3.5 h-3.5 mt-0.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2} style={{ color: "#ef4444" }}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                      <p className="text-[11px] leading-relaxed" style={{ color: "var(--text-primary)" }}>
                        确定要清除所有对话数据吗？此操作不可撤销，所有对话和消息将被永久删除。
                      </p>
                    </div>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => setShowClearConfirm(false)}
                        disabled={clearing}
                        className="px-3 py-1.5 rounded-md text-[11px] font-medium transition-colors duration-150"
                        style={{
                          background: "var(--surface-raised)",
                          color: "var(--text-secondary)",
                        }}
                      >
                        取消
                      </button>
                      <button
                        onClick={handleClearData}
                        disabled={clearing}
                        className="px-3 py-1.5 rounded-md text-[11px] font-semibold transition-all duration-150 flex items-center gap-1.5"
                        style={{
                          background: "#ef4444",
                          color: "#fff",
                          opacity: clearing ? 0.6 : 1,
                        }}
                      >
                        {clearing ? (
                          <>
                            <svg className="w-3 h-3 animate-spin" fill="none" viewBox="0 0 24 24">
                              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                            </svg>
                            清除中...
                          </>
                        ) : (
                          "确认清除"
                        )}
                      </button>
                    </div>
                  </div>
                ) : (
                  <button
                    onClick={() => setShowClearConfirm(true)}
                    className="settings-clear-btn"
                    style={{
                      borderColor: "rgba(239, 68, 68, 0.25)",
                    }}
                  >
                    <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                    </svg>
                    清除所有对话数据
                  </button>
                )}
              </div>
            </motion.section>
          ) : (
            <motion.section
              key="appearance"
              initial={{ opacity: 0, x: 12 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 12 }}
              transition={{ duration: 0.18, ease: "easeOut" }}
              className="space-y-3 mt-3"
            >
              <AppearanceSettingsTab localConfig={localConfig} patch={patch} />
            </motion.section>
          )}
        </AnimatePresence>
      </div>

      {/* 底部按钮 */}
      <div className="settings-footer">
        <button
          onClick={handleSave}
          disabled={saving || !hasChanges || !!aiValidationError || !!bubbleValidationError}
          className="w-full py-2 rounded-lg text-sm font-semibold transition-all duration-150 disabled:cursor-not-allowed"
          style={{
            background: saving || !hasChanges || aiValidationError || bubbleValidationError ? "var(--surface-hover)" : "var(--accent)",
            color: saving || !hasChanges || aiValidationError || bubbleValidationError ? "var(--text-secondary)" : "white",
            opacity: saving || !hasChanges || aiValidationError || bubbleValidationError ? 0.72 : 1,
          }}
        >
          {saving ? "保存中..." : aiValidationError ? "请完善 AI 配置" : bubbleValidationError ? "请检查自定义时间" : hasChanges ? "保存配置" : "无需保存"}
        </button>
      </div>
    </div>
  );
}
