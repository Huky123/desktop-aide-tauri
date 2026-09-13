/** 附件 —— 图片或文件 */
export type Attachment = ImageAttachment | FileAttachment;

export interface ImageAttachment {
  type: "image";
  id: string;
  /** 原始 base64 数据（不含 data: 前缀）；URL 图片可为空字符串 */
  data: string;
  /** 本地 asset 或远程图片地址，用于预览、放大和下载。 */
  url?: string;
  /** MIME 类型，如 "image/png" */
  mimeType: string;
  /** 文件名或自动生成名 */
  name: string;
  /** 字节数（估算值） */
  size: number;
  /** 图片宽度（如有） */
  width?: number;
  /** 图片高度（如有） */
  height?: number;
  /** 来源：剪贴板 / 文件 / 截图 / AI 生成 */
  source: "clipboard" | "file" | "screenshot" | "ai-generated";
}

export interface FileAttachment {
  type: "file";
  id: string;
  /** 原始文件名 */
  name: string;
  /** 文件大小（字节） */
  size: number;
  /** MIME 类型 */
  mimeType: string;
  /** 文本文件内容；旧历史记录可能没有此字段 */
  data?: string;
  /** 内容是否因上下文保护而截断 */
  truncated?: boolean;
}

/** 发送给后端的附件 DTO */
export interface AttachmentDto {
  data: string;
  mime_type: string;
  name: string;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
  /** 附件列表（图片/文件），仅用户消息携带 */
  attachments?: Attachment[];
  /** 后端持久化图片 ID 到本地绝对路径的映射。 */
  localImagePaths?: Record<string, string>;
}

export interface AppConfig {
  ai_provider:
    | "anthropic"
    | "openai"
    | "xai"
    | "gemini"
    | "deepseek"
    | "qwen"
    | "kimi"
    | "zhipu"
    | "openrouter"
    | "ollama"
    | "custom";
  model: string;
  api_key: string;
  api_base: string;
  max_tokens: number;
  temperature: number;
  ollama_endpoint: string;
  /** Ollama 默认模型名（与 model 字段同步，供后端兼容旧前端） */
  ollama_model: string;
  bg_color: string;
  bg_opacity: number;
  accent_color: string;
  msg_user_bg: string;
  msg_user_border: string;
  panel_width: number;
  panel_height: number;
  theme_mode: "auto" | "dark" | "light";
  theme_preset: "indigo" | "midnight" | "emerald" | "cloud" | "sakura" | "moss" | "custom";
  /** 气泡自动折叠开关 */
  bubble_auto_collapse: boolean;
  /** 气泡自动折叠延迟（秒），面板关闭后多久自动收折 */
  bubble_collapse_delay: number;
  /** 工具调用总开关（设置 → 通用：启用工具调用） */
  enable_tools: boolean;
  /** 允许 AI 联网搜索（AI 按需自动调用 web_search 工具） */
  enable_web_search: boolean;
  /** Tavily 搜索 API Key（可选；为空时自动退回 DuckDuckGo 免 key 搜索） */
  tavily_api_key: string;
  /**
   * 图片输入方式（"能否识图"维度，与"能否出图"相互独立）：
   * - `off`（默认）：走 OCR 文字识别
   * - `on`：支持识图，图片直接发给模型
   *
   * 设置界面是二选一；`auto`（按模型能力注册表判断）只是旧配置的遗留取值，
   * 后端加载时会解析成 `on`/`off`，前端 normalize 也会兜底转换。
   */
  vision_mode: "off" | "auto" | "on";
  /**
   * 模型用途（只描述"能否出图"，避免模型名启发式误判）：
   * - `chat`（默认）：不是出图模型，保持流式
   * - `image`：是出图模型，走非流式通路（工具与联网搜索仍然可用）
   *
   * 设置界面是二选一；`auto`（按模型名判断）只是旧配置的遗留取值，
   * 后端加载时会解析成 `image`/`chat`。
   *
   * 与 `vision_mode` 互不排斥：既能识图又能出图的模型（如
   * gemini-2.5-flash-image）应同时开启两项。
   */
  model_kind: "auto" | "chat" | "image";
}

/** 后端返回的原始配置（所有字段可选，需 normalize 后再使用） */
export type RawConfigDto = Partial<AppConfig>;

/** 前端默认配置（后端无配置时使用） */
export const DEFAULT_CONFIG: Readonly<AppConfig> = Object.freeze({
  // 默认 DeepSeek：面向国内用户，Key 门槛最低（与后端 default_provider 保持一致）
  ai_provider: "deepseek",
  model: "deepseek-chat",
  api_key: "",
  api_base: "",
  max_tokens: 4096,
  temperature: 0.7,
  ollama_endpoint: "http://localhost:11434",
  ollama_model: "llama3.2",
  bg_color: "#121216",
  bg_opacity: 0.72,
  accent_color: "#818cf8",
  msg_user_bg: "",
  msg_user_border: "",
  panel_width: 420,
  panel_height: 600,
  theme_mode: "auto",
  theme_preset: "indigo",
  bubble_auto_collapse: false,
  bubble_collapse_delay: 300,
  enable_tools: false,
  enable_web_search: false,
  tavily_api_key: "",
  vision_mode: "off",
  model_kind: "chat",
});

/** 对话列表项（来自后端） */
export interface ConversationInfo {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
  preview: string;
  message_count: number;
}

export interface AiStreamChunkEvent {
  request_id: string;
  chunk: string;
}

export interface AiStreamDoneEvent {
  request_id: string;
}

/** 存储位置信息（设置页展示） */
export interface StorageInfo {
  /** 当前数据目录 */
  data_dir: string;
  /** 默认数据目录 */
  default_dir: string;
  /** 是否使用了自定义位置 */
  is_custom: boolean;
}

/** 截图结果（后端 capture_screen 返回） */
export interface CaptureScreenResult {
  /** PNG base64（不含 data: 前缀） */
  image_base64: string;
  /** 物理像素宽度 */
  width: number;
  /** 物理像素高度 */
  height: number;
}

/** 截图窗口提交的裁剪附件 DTO */
export interface ScreenshotAttachmentDto {
  data: string;
  mimeType: string;
  name: string;
  width: number;
  height: number;
}

/** 消息搜索结果（后端 search_messages 返回） */
export interface SearchResult {
  conversation_id: string;
  title: string;
  preview: string;
  timestamp: number;
}

/** 定时提醒条目 */
export interface ReminderInfo {
  id: string;
  /** 触发时间（毫秒时间戳） */
  remind_at: number;
  content: string;
  created_at: number;
}

/** 提醒触发事件载荷（后端 broadcast `reminder-fired`，方案 A：气泡脉冲 + 面板内助手消息） */
export interface ReminderFiredEvent {
  content: string;
  /** 触发时间（毫秒时间戳） */
  fired_at: number;
}

/**
 * 「测试连接」所需的最小连接信息（与后端 `ConnectionDto` 一一对应）。
 *
 * 刻意不传整套 `AppConfig`：探测只关心地址/Key/模型，传子集可以避免
 * 因为配色、面板尺寸等无关字段而反序列化失败。
 */
export interface ConnectionInput {
  ai_provider: string;
  model: string;
  api_key: string;
  api_base: string;
  ollama_endpoint: string;
}

/** 一条探测结论 */
export interface ProbeCheck {
  /** `key` / `models` / `model_listed` / `chat` / `images_route` */
  id: string;
  label: string;
  status: "ok" | "warn" | "fail";
  detail: string;
}

/** 「测试连接」的完整体检表 */
export interface ProbeReport {
  checks: ProbeCheck[];
  /** `GET /models` 拉到的可用模型（部分网关不实现该端点，此时为空） */
  models: string[];
  /** 后端建议的「模型用途」；无法判断时为 null */
  suggested_model_kind: "chat" | "image" | null;
}

/**
 * 「获取可用模型」的结果。
 *
 * **必须区分「拿到了空列表」与「根本没拿到」**：大多数网关不实现 `/models`，
 * 若只回空数组，用户点了按钮毫无反馈，只会以为按钮坏了。
 */
export interface ModelListResult {
  models: string[];
  /**
   * 失败原因，成功时为 `null`：
   * - `no_base`：还没填接口地址
   * - `auth`：API Key 无效
   * - `no_endpoint`：该服务商不提供模型列表接口（**最常见**）
   * - `network`：连不上该地址
   */
  reason: "no_base" | "auth" | "no_endpoint" | "network" | null;
}
