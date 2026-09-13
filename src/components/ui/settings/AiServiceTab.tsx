import { useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { motion } from "framer-motion";
import { FormField } from "../FormField";
import { PROVIDERS } from "../settingsOptions";
import { BinaryChoice } from "./BinaryChoice";
import { tauriApi } from "../../../services/tauriApi";
import type { AppConfig, ConnectionInput, ProbeReport } from "../../../types";

/**
 * 「获取可用模型」没拿到列表时的说明。
 *
 * 原先失败时只把列表清空、一个字都不说，用户点了按钮毫无反馈，只会以为按钮坏了
 * ——而「该服务商不实现 /models」恰恰是最常见的情况。
 */
const MODELS_EMPTY_MESSAGE: Record<string, string> = {
  no_base: "请先填写接口地址",
  auth: "API Key 无效，读不到模型列表",
  no_endpoint: "该服务商不提供模型列表，请手动填写模型名",
  network: "连不上这个地址，请检查地址或网络",
  empty: "该服务商没有返回模型列表，请手动填写模型名",
};

export interface AiServiceTabProps {
  /** 设置面板的本地草稿配置（保存前的编辑副本） */
  localConfig: AppConfig;
  /** 合并写入草稿的某个字段 */
  patch: (partial: Partial<AppConfig>) => void;
  /**
   * 记住当前服务商已填写的字段——切换服务商时用于把该服务商原来的值回填。
   *
   * 这里传回调而不是直接传 ref：ref 作为 props 被写入会触发
   * react-hooks/immutability（不得修改组件 props）。
   */
  rememberProviderField: (field: "model" | "api_key" | "api_base", value: string) => void;
  showAdvanced: boolean;
  setShowAdvanced: Dispatch<SetStateAction<boolean>>;
  /** 非空字符串表示当前 AI 配置不完整（禁止保存） */
  aiValidationError: string;
  handleProviderChange: (provider: AppConfig["ai_provider"]) => void;
}

/**
 * 设置面板「AI 服务」页内容。
 *
 * 不含外层动画容器——`motion.section` 由 SettingsPanel 持有，
 * 以保证 AnimatePresence 的切页动画与 key 完全不变。
 */
export function AiServiceTab({
  localConfig,
  patch,
  rememberProviderField,
  showAdvanced,
  setShowAdvanced,
  aiValidationError,
  handleProviderChange,
}: AiServiceTabProps) {
  /** `GET /models` 拉到的候选模型，供输入框的 datalist 联想（手填始终可用） */
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  /** 拉取没拿到列表时的一句说明（成功或还没拉取时为空） */
  const [modelsMessage, setModelsMessage] = useState("");
  const [probing, setProbing] = useState(false);
  const [probeReport, setProbeReport] = useState<ProbeReport | null>(null);
  const [probeError, setProbeError] = useState("");

  /** 只发连接相关的字段（后端 `ConnectionDto`） */
  const connection = (): ConnectionInput => ({
    ai_provider: localConfig.ai_provider,
    model: localConfig.model,
    api_key: localConfig.api_key,
    api_base: localConfig.api_base,
    ollama_endpoint: localConfig.ollama_endpoint,
  });

  const fetchModels = async () => {
    setModelsLoading(true);
    setModelsMessage("");
    try {
      const result = await tauriApi.listAiModels(connection());
      setAvailableModels(result.models);
      if (result.models.length === 0) {
        // 拉不到不是错误，但**必须说清楚**：大多数服务商不实现这个接口，
        // 默不作声的话用户只会以为按钮坏了
        setModelsMessage(MODELS_EMPTY_MESSAGE[result.reason ?? "empty"]);
      }
    } catch (error) {
      setAvailableModels([]);
      setModelsMessage(`拉取失败：${String(error)}`);
    } finally {
      setModelsLoading(false);
    }
  };

  const runProbe = async () => {
    setProbing(true);
    setProbeError("");
    setProbeReport(null);
    try {
      const report = await tauriApi.probeAiConfig(connection());
      setProbeReport(report);
      if (report.models.length > 0) {
        setAvailableModels(report.models);
        setModelsMessage("");
      }
    } catch (error) {
      setProbeError(String(error));
    } finally {
      setProbing(false);
    }
  };

  const currentKind = localConfig.model_kind === "image" ? "image" : "chat";
  const suggestedKind =
    probeReport?.suggested_model_kind && probeReport.suggested_model_kind !== currentKind
      ? probeReport.suggested_model_kind
      : null;

  return (
    <>
              {/* AI 提供商 */}
              <div>
                <label className="text-[11px] mb-1 block" style={{ color: "var(--text-secondary)" }}>
                  AI 服务商
                </label>
                <div className="provider-select-wrap">
                  <span
                    className="provider-select-dot"
                    style={{ background: PROVIDERS.find((p) => p.value === localConfig.ai_provider)?.brandColor }}
                  />
                  <select
                    className="provider-select"
                    value={localConfig.ai_provider}
                    onChange={(event) => handleProviderChange(event.target.value as AppConfig["ai_provider"])}
                  >
                    {PROVIDERS.map((provider) => (
                      <option key={provider.value} value={provider.value}>{provider.label}</option>
                    ))}
                  </select>
                  <svg className="provider-select-chevron" viewBox="0 0 24 24" aria-hidden="true"><path d="m7 10 5 5 5-5" /></svg>
                </div>
              </div>

              <div>
                <FormField
                  label="模型名称"
                  value={localConfig.model}
                  onChange={(v) => {
                    patch({ model: v });
                    rememberProviderField("model", v);
                  }}
                  placeholder="例如: deepseek-chat"
                  inputProps={{ list: "ai-model-options" }}
                />
                <div className="flex items-center gap-2 mt-1">
                  <button
                    type="button"
                    className="settings-notice-action"
                    onClick={fetchModels}
                    disabled={modelsLoading}
                  >
                    {modelsLoading ? "拉取中…" : "获取可用模型"}
                  </button>
                  <span className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>
                    {availableModels.length > 0
                      ? `已获取 ${availableModels.length} 个，点输入框可直接选`
                      : ""}
                  </span>
                </div>
                {modelsMessage && (
                  <p className="settings-inline-warn" role="status">
                    {modelsMessage}
                  </p>
                )}
                <datalist id="ai-model-options">
                  {availableModels.map((name) => (
                    <option key={name} value={name} />
                  ))}
                </datalist>
              </div>

              {/* 「出图模型」这个标记是黏的：它作用于当前模型，换模型时不会自动取消，
                  而它会改变**所有**请求的去向。常驻显示状态，避免"换了模型却仍被
                  打发出图接口"这种静默失败（真实故障：照错误提示换了模型，依然失败）。 */}

              {/* 识图能力：与"出图"相互独立 */}
              <BinaryChoice
                label="模型识图方式"
                value={localConfig.vision_mode === "on" ? "on" : "off"}
                onChange={(value) => patch({ vision_mode: value })}
                options={[
                  {
                    value: "off",
                    label: "OCR 识别文字",
                    desc: "图片不发给模型，用本地 OCR 把图中文字提取出来注入上下文，纯文本模型也能用。",
                  },
                  {
                    value: "on",
                    label: "模型识图",
                    desc: "模型本身支持识图时选它，图像理解更准确",
                  },
                ]}
              />

              {/* 出图能力：与"识图"相互独立 */}
              <BinaryChoice
                label="模型出图能力"
                value={currentKind}
                onChange={(value) => patch({ model_kind: value })}
                options={[
                  {
                    value: "chat",
                    label: "不是出图模型",
                    desc: "",
                  },
                  {
                    value: "image",
                    label: "是出图模型",
                    desc: "",
                  },
                ]}
              />

              {/* 测试连接：把"发一条消息试试"变成一次点击。
                  配置链路原本没有任何验证环节，用户只能靠试错发现问题。 */}
              <div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    className="settings-notice-action"
                    onClick={runProbe}
                    disabled={probing}
                  >
                    {probing ? "检测中…" : "测试连接"}
                  </button>
                  <span className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>
                    检查 Key、模型名与两条端点
                  </span>
                </div>

                {probeError && (
                  <p className="settings-validation" role="alert">
                    检测失败：{probeError}
                  </p>
                )}

                {probeReport && (
                  <ul className="settings-probe-list">
                    {probeReport.checks.map((item) => (
                      <li key={item.id} data-status={item.status}>
                        <span className="settings-probe-mark" aria-hidden="true">
                          {item.status === "ok" ? "✓" : item.status === "warn" ? "!" : "✕"}
                        </span>
                        <span>
                          <strong>{item.label}</strong>
                          <span className="settings-probe-detail">{item.detail}</span>
                        </span>
                      </li>
                    ))}
                  </ul>
                )}

                {suggestedKind && (
                  <div className="settings-notice" role="status">
                    <span>
                      检测建议把「模型用途」改为
                      {suggestedKind === "image" ? "「是出图模型」" : "「不是出图模型」"}。
                    </span>
                    <button
                      type="button"
                      className="settings-notice-action"
                      onClick={() => patch({ model_kind: suggestedKind })}
                    >
                      应用
                    </button>
                  </div>
                )}
              </div>

              {localConfig.ai_provider !== "ollama" && (
                <FormField
                  label="API Key"
                  type="password"
                  value={localConfig.api_key}
                  onChange={(v) => {
                    patch({ api_key: v });
                    rememberProviderField("api_key", v);
                  }}
                  placeholder="sk-..."
                />
              )}

              {localConfig.ai_provider === "ollama" && (
                <FormField
                  label="Ollama 地址"
                  value={localConfig.ollama_endpoint}
                  onChange={(v) => patch({ ollama_endpoint: v })}
                  placeholder="http://localhost:11434"
                />
              )}

              {localConfig.ai_provider === "custom" && (
                <FormField
                  label="兼容 API 地址"
                  value={localConfig.api_base}
                  onChange={(v) => {
                    patch({ api_base: v });
                    rememberProviderField("api_base", v);
                  }}
                  placeholder="https://example.com/v1"
                  hint="需兼容 OpenAI Chat Completions API"
                />
              )}

              {aiValidationError && (
                <p className="settings-validation" role="alert">{aiValidationError}</p>
              )}

              {/* 高级参数 — 可折叠 */}
              <button
                onClick={() => setShowAdvanced(!showAdvanced)}
                className="settings-disclosure"
                style={{ color: "var(--text-tertiary)" }}
                aria-expanded={showAdvanced}
              >
                <span>高级参数</span>
                <motion.span
                  animate={{ rotate: showAdvanced ? 180 : 0 }}
                  transition={{ duration: 0.15 }}
                  style={{ display: "inline-flex" }}
                >
                  ▼
                </motion.span>
              </button>
              <motion.div
                animate={{
                  height: showAdvanced ? "auto" : 0,
                  opacity: showAdvanced ? 1 : 0,
                }}
                transition={{ duration: 0.2, ease: "easeInOut" }}
                className="overflow-hidden"
              >
                <div className="settings-advanced space-y-3">
                  {localConfig.ai_provider !== "custom" && (
                    <FormField
                      label="API 地址"
                      hint="(留空使用默认)"
                      value={localConfig.api_base}
                      onChange={(v) => {
                        patch({ api_base: v });
                        rememberProviderField("api_base", v);
                      }}
                      placeholder={
                        localConfig.ai_provider === "ollama"
                          ? "http://localhost:11434/v1"
                          : "留空使用所选服务商默认地址"
                      }
                    />
                  )}
                  <div className="grid grid-cols-2 gap-2">
                    <FormField
                      label="Temperature"
                      type="number"
                      value={localConfig.temperature}
                      onChange={(v) => {
                        const n = parseFloat(v);
                        patch({ temperature: isNaN(n) ? 0.7 : n });
                      }}
                      inputProps={{ min: 0, max: 2, step: 0.1 }}
                    />
                    <FormField
                      label="Max Tokens"
                      type="number"
                      value={localConfig.max_tokens}
                      onChange={(v) => {
                        const n = parseInt(v);
                        patch({ max_tokens: isNaN(n) ? 4096 : n });
                      }}
                      inputProps={{ min: 256, max: 32768, step: 256 }}
                    />
                  </div>
                </div>
              </motion.div>
    </>
  );
}
