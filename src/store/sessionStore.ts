import { create } from "zustand";
import type { ChatMessage, Attachment, ImageAttachment, ConversationInfo } from "../types";

interface SessionState {
  messages: ChatMessage[];
  isAiResponding: boolean;
  activeAiRequestId: string | null;
  streamingText: string;
  /** 输入框中待发送的附件 */
  pendingAttachments: Attachment[];
  /** 灯箱当前显示的图片 */
  lightboxImage: ImageAttachment | null;
  /** 当前对话 ID */
  currentConversationId: string;
  /** 对话列表 */
  conversations: ConversationInfo[];

  addMessage: (msg: ChatMessage) => void;
  replaceMessage: (msg: ChatMessage) => void;
  clearMessages: () => void;
  restoreMessages: (msgs: ChatMessage[]) => void;
  setAiResponding: (responding: boolean) => void;
  setActiveAiRequestId: (requestId: string | null) => void;
  appendStreamingText: (chunk: string) => void;
  clearStreamingText: () => void;
  commitStreamToMessage: (msgContent?: string, suffix?: string) => void;
  addPendingAttachment: (att: Attachment) => void;
  removePendingAttachment: (id: string) => void;
  clearPendingAttachments: () => void;
  openLightbox: (img: ImageAttachment) => void;
  closeLightbox: () => void;
  setCurrentConversationId: (id: string) => void;
  setConversations: (list: ConversationInfo[]) => void;
  addConversation: (conv: ConversationInfo) => void;
  removeConversation: (id: string) => void;
  updateConversation: (id: string, updates: Partial<ConversationInfo>) => void;
}

// 行缓冲式流式处理：按换行符边界 flush，避免在 markdown 语法中间截断
// 使用 Map 按 requestId 隔离状态，避免多个流式响应互相干扰
const _streamBuffers = new Map<string, string>();
const _flushTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** 单个 buffer 最大字节数（64KB），超限强制 flush 防止单行过长撑爆内存 */
const MAX_BUFFER_SIZE = 64 * 1024;
/** 最大并发 stream 缓冲区数量，超限淘汰最旧条目防止 Map 泄漏 */
const MAX_STREAM_ENTRIES = 10;
/**
 * 流式刷新间隔（毫秒）。
 *
 * 之前的实现每来一个 chunk 就把 200ms 兜底定时器清掉重设，导致「连续 chunk 但长时间
 * 没有换行」时（长段落、单行 JSON）文本迟迟不刷新，一停就猛跳一大段。现在定时器
 * 只设一次、不被后续 chunk 推迟，最多 100ms 一定把缓冲区（含未成行的尾部）刷出来；
 * 同时把高频 chunk 合并到 10 次/秒级别，减少 Markdown 重解析。
 */
const STREAM_FLUSH_INTERVAL_MS = 100;

/** 清理指定 requestId 的所有缓冲区资源 */
function _cleanupStreamEntry(requestId: string) {
  const timer = _flushTimers.get(requestId);
  if (timer) {
    clearTimeout(timer);
    _flushTimers.delete(requestId);
  }
  _streamBuffers.delete(requestId);
}

/** 淘汰最旧的缓冲区条目（当超过 MAX_STREAM_ENTRIES 时） */
function _evictOldestIfNeeded() {
  while (_streamBuffers.size > MAX_STREAM_ENTRIES) {
    const oldestKey = _streamBuffers.keys().next().value;
    if (oldestKey) _cleanupStreamEntry(oldestKey);
  }
}

/** 将缓冲区中的余留文本刷入 streamingText（模块级，被 append/commit/clear 共用） */
function _flushStreaming(
  requestId: string,
  set: (fn: (s: SessionState) => Partial<SessionState>) => void
) {
  const buffer = _streamBuffers.get(requestId) || "";
  if (buffer.length === 0) return;
  _streamBuffers.delete(requestId);
  set((s) => {
    // 只在 requestId 匹配时才更新（避免过期请求的 flush）
    if (s.activeAiRequestId === requestId) {
      return { streamingText: s.streamingText + buffer };
    }
    return {};
  });
}

/**
 * 确保该 requestId 有一个待触发的刷新定时器；**已存在则不重置**。
 * 这样无论 chunk 多密集，最多 STREAM_FLUSH_INTERVAL_MS 后一定会刷出缓冲区，
 * 不会出现"连续 chunk 把定时器不断推后 → 文本长时间不动再猛跳"。
 */
function _scheduleFlush(
  requestId: string,
  set: (fn: (s: SessionState) => Partial<SessionState>) => void
) {
  if (_flushTimers.has(requestId)) return;
  const timer = setTimeout(() => {
    _flushTimers.delete(requestId);
    _flushStreaming(requestId, set);
  }, STREAM_FLUSH_INTERVAL_MS);
  _flushTimers.set(requestId, timer);
}

/** 取消指定 requestId 的待触发刷新定时器 */
function _clearFlushTimer(requestId: string) {
  const timer = _flushTimers.get(requestId);
  if (timer) {
    clearTimeout(timer);
    _flushTimers.delete(requestId);
  }
}

export const useSessionStore = create<SessionState>((set) => ({
  messages: [],
  isAiResponding: false,
  activeAiRequestId: null,
  streamingText: "",
  pendingAttachments: [],
  lightboxImage: null,
  currentConversationId: "default",
  conversations: [],

  addMessage: (msg) => set((s) => ({ messages: [...s.messages, msg] })),
  replaceMessage: (msg) => set((s) => ({
    messages: s.messages.map((current) => current.id === msg.id ? msg : current),
  })),
  clearMessages: () => set({ messages: [] }),
  restoreMessages: (msgs) => set({ messages: msgs }),
  setAiResponding: (r) => set({ isAiResponding: r }),
  setActiveAiRequestId: (requestId) =>
    set((s) => {
      // 切换请求时清理旧 requestId 的缓冲区，防止 Map 内存泄漏
      const oldId = s.activeAiRequestId;
      if (oldId && oldId !== requestId) {
        _cleanupStreamEntry(oldId);
      }
      // 确保新 requestId 的缓冲区从干净状态开始
      if (requestId) {
        _cleanupStreamEntry(requestId);
      }
      _evictOldestIfNeeded();
      return { activeAiRequestId: requestId };
    }),
  appendStreamingText: (chunk) => {
    set((s) => {
      const requestId = s.activeAiRequestId;
      if (!requestId) return {};

      const currentBuffer = _streamBuffers.get(requestId) || "";
      const newBuffer = currentBuffer + chunk;

      // 缓冲区大小保护：超限时立即整体刷出，避免单行长文本撑爆内存
      if (newBuffer.length > MAX_BUFFER_SIZE) {
        _streamBuffers.delete(requestId);
        _clearFlushTimer(requestId);
        return { streamingText: s.streamingText + newBuffer };
      }

      // 按换行符边界 flush：完整行立即显示（保留 markdown 行结构）
      const lastNewline = newBuffer.lastIndexOf("\n");
      if (lastNewline !== -1) {
        const completeLines = newBuffer.slice(0, lastNewline + 1);
        const remainder = newBuffer.slice(lastNewline + 1);
        _streamBuffers.set(requestId, remainder);
        if (remainder.length > 0) {
          // 尾行可能不完整，交给定时器（≤100ms 后也会显示）
          _scheduleFlush(requestId, set);
        } else {
          _clearFlushTimer(requestId);
        }
        return { streamingText: s.streamingText + completeLines };
      }

      // 未成行：不重置已有定时器，最多 100ms 后把尾行也刷出来
      _streamBuffers.set(requestId, newBuffer);
      _scheduleFlush(requestId, set);
      return {};
    });
  },
  clearStreamingText: () => {
    set((s) => {
      const requestId = s.activeAiRequestId;
      if (requestId) {
        _cleanupStreamEntry(requestId);
      }
      return { streamingText: "" };
    });
  },
  commitStreamToMessage: (msgContent, suffix = "") => {
    return set((s) => {
      const requestId = s.activeAiRequestId;
      // 直接从缓冲区读取余留文本（不嵌套 set，避免覆盖）
      let flushedBuffer = "";
      if (requestId) {
        const timer = _flushTimers.get(requestId);
        if (timer) {
          clearTimeout(timer);
          _flushTimers.delete(requestId);
        }
        flushedBuffer = _streamBuffers.get(requestId) || "";
        _streamBuffers.delete(requestId);
      }

      const streamedContent = msgContent ?? (s.streamingText + flushedBuffer);
      if (!streamedContent) return { isAiResponding: false, activeAiRequestId: null };
      const content = streamedContent + suffix;
      const msg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content,
        timestamp: Date.now(),
      };
      return {
        messages: [...s.messages, msg],
        streamingText: "",
        isAiResponding: false,
        activeAiRequestId: null,
      };
    });
  },
  addPendingAttachment: (att) =>
    set((s) => ({ pendingAttachments: [...s.pendingAttachments, att] })),
  removePendingAttachment: (id) =>
    set((s) => ({ pendingAttachments: s.pendingAttachments.filter((a) => a.id !== id) })),
  clearPendingAttachments: () => set({ pendingAttachments: [] }),
  openLightbox: (img) => set({ lightboxImage: img }),
  closeLightbox: () => set({ lightboxImage: null }),
  setCurrentConversationId: (id) => set({ currentConversationId: id }),
  setConversations: (list) => set({ conversations: list }),
  addConversation: (conv) =>
    set((s) => ({ conversations: [conv, ...s.conversations] })),
  removeConversation: (id) =>
    set((s) => ({ conversations: s.conversations.filter((c) => c.id !== id) })),
  updateConversation: (id, updates) =>
    set((s) => ({
      conversations: s.conversations.map((c) =>
        c.id === id ? { ...c, ...updates } : c,
      ),
    })),
}));
