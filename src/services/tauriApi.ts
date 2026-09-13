import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  AttachmentDto,
  CaptureScreenResult,
  ChatMessage,
  ConnectionInput,
  ConversationInfo,
  ModelListResult,
  ProbeReport,
  ReminderInfo,
  ScreenshotAttachmentDto,
  SearchResult,
  StorageInfo,
} from "../types";
import {
  resolveLocalImageRefs,
  resolveLocalImages,
  restoreLocalImageRefs,
} from "../lib/localImages";

/**
 * Tauri 后端 API 服务层
 * 集中封装所有 invoke 调用，组件不直接依赖 Tauri command 名称。
 * 测试时 mock 此模块即可，无需 mock @tauri-apps/api/core。
 */
export const tauriApi = {
  getConfig: () => invoke<AppConfig>("get_config"),

  saveConfig: (config: AppConfig) =>
    invoke("save_config", { configDto: config }),

  /**
   * 拉取可用模型清单（供模型下拉）。
   *
   * 返回 `models` + `reason`：拉不到不是错误，但**必须能说明原因**——
   * 否则用户点了按钮没有任何反馈。
   */
  listAiModels: (connection: ConnectionInput) =>
    invoke<ModelListResult>("list_ai_models", { connection }),

  /** 测试连接：Key / 模型清单 / 对话端点 / 出图路由四项探测（不产生出图费用） */
  probeAiConfig: (connection: ConnectionInput) =>
    invoke<ProbeReport>("probe_ai_config", { connection }),

  togglePanel: (expand: boolean) =>
    invoke("toggle_panel", { expand }),

  savePanelSize: (width: number, height: number) =>
    invoke("save_panel_size", { width, height }),

  quitApp: () => invoke("quit_app"),

  showBubbleMenu: () => invoke("show_bubble_menu"),

  aiChat: (requestId: string, message: string, images?: AttachmentDto[]) =>
    invoke("ai_chat", { requestId, message, images }),

  /** 停止当前 AI 生成（保留已收到的内容） */
  stopGeneration: () =>
    invoke("stop_generation"),

  resetConversation: () =>
    invoke("reset_conversation"),

  isForegroundFullscreen: () =>
    invoke<boolean>("is_foreground_fullscreen"),

  /** 全屏截图：隐藏本应用窗口抓屏后，打开独立全屏截图窗口选框 */
  captureScreen: () =>
    invoke<void>("capture_screen"),

  /** 截图窗口拉取截图数据 */
  getScreenshotData: () =>
    invoke<CaptureScreenResult>("get_screenshot_data"),

  /** 截图窗口提交裁剪结果（后端广播给主窗口并关闭截图窗口） */
  submitScreenshotCapture: (attachment: ScreenshotAttachmentDto) =>
    invoke("submit_screenshot_capture", { attachment }),

  /** 取消截图并关闭截图窗口 */
  cancelScreenshot: () =>
    invoke("cancel_screenshot"),

  // ── 聊天记录持久化 ──

  /** 保存单条消息到数据库 */
  saveMessage: (msg: ChatMessage, convId?: string) =>
    invoke<ChatMessage>("save_message", {
      msg: restoreLocalImageRefs(msg),
      convId: convId ?? null,
    })
      .then(resolveLocalImageRefs),

  /** 清除当前会话的数据库记录 */
  clearHistoryMessages: (convId?: string) =>
    invoke("clear_history_messages", { convId: convId ?? null }),

  // ── 多对话管理 ──

  /** 创建新对话 */
  createConversation: (title?: string) =>
    invoke<ConversationInfo>("create_conversation", { title: title ?? null }),

  /** 列出所有对话 */
  listConversations: () =>
    invoke<ConversationInfo[]>("list_conversations"),

  /** 按关键词搜索所有会话的消息 */
  searchMessages: (query: string) =>
    invoke<SearchResult[]>("search_messages", { query }),

  /** 用 AI 为会话生成标题（主请求忙时静默跳过） */
  generateConversationTitle: (convId: string) =>
    invoke<string | null>("generate_conversation_title", { convId }),

  /** 重命名对话 */
  renameConversation: (convId: string, newTitle: string) =>
    invoke("rename_conversation", { convId, newTitle }),

  /** 删除对话 */
  deleteConversation: (convId: string) =>
    invoke("delete_conversation", { convId }),

  /** 切换活跃对话，返回目标对话的消息列表 */
  switchConversation: (convId: string) =>
    invoke<ChatMessage[]>("switch_conversation", { convId }).then(resolveLocalImages),

  /** 撤回指定消息及之后的所有消息，返回剩余消息列表 */
  retractMessage: (msgId: string) =>
    invoke<ChatMessage[]>("retract_message", { msgId }).then(resolveLocalImages),

  /** 清除所有对话数据（消息 + 对话元数据），重置为初始状态 */
  clearAllData: () =>
    invoke("clear_all_data"),

  // ── 存储位置 ──

  /** 查询当前数据存储位置 */
  getStorageInfo: () =>
    invoke<StorageInfo>("get_storage_info"),

  /** 迁移数据到新目录（后端完成复制 + 更新注册表 + 重启应用） */
  moveDataDir: (target: string) =>
    invoke("move_data_dir", { target }),

  /** 将 base64 图片数据写入用户选择的路径（灯箱"保存到…"） */
  saveImageToFile: (data: string, mimeType: string, path: string) =>
    invoke("save_base64_image", { data, mimeType, path }),

  // ── 定时提醒 ──

  /** 创建提醒：timeExpr 支持「10分钟」「2小时」「14:30」等 */
  createReminder: (timeExpr: string, content: string) =>
    invoke<ReminderInfo>("create_reminder", { timeExpr, content }),
};
