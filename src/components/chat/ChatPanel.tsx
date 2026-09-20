import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { toast } from "sonner";
import { useUiStore } from "../../store/uiStore";
import { useSessionStore } from "../../store/sessionStore";
import { useConfigStore } from "../../store/configStore";
import { hexToRgba } from "../../lib/color";
import { useTauriEvent } from "../../hooks/useTauriEvent";
import { useThinkingAnimation } from "../../hooks/useThinkingAnimation";
import { useChatActions } from "../../hooks/useChatActions";
import { tauriApi } from "../../services/tauriApi";
import { persistMessage } from "../../services/chat/messagePersistence";
import {
  refreshConversationList,
  createAndSwitchConversation,
  switchToConversation,
  deleteConversationAndHandle,
  renameConversation,
} from "../../services/chat/conversationService";
import type {
  AiStreamChunkEvent,
  AiStreamDoneEvent,
  ImageAttachment,
  ScreenshotAttachmentDto,
} from "../../types";
import { SettingsPanel } from "../ui/SettingsPanel";
import { HeaderToolbar } from "../ui/HeaderToolbar";
import { Lightbox } from "../ui/Lightbox";
import { ConversationList } from "./ConversationList";
import { MessageList } from "./MessageList";
import { InputArea } from "./InputArea";

const PANEL_STYLE = {
  top: 0,
  bottom: 0,
  borderRadius: 8,
  border: "1px solid var(--panel-border)",
};

export function ChatPanel() {
  // ── Store 订阅 ──
  const isPanelOpen = useUiStore((s) => s.isPanelOpen);
  const showSettings = useUiStore((s) => s.showSettings);
  const config = useConfigStore((s) => s.config);

  const isAiResponding = useSessionStore((s) => s.isAiResponding);
  // 只订阅布尔值：完整 streamingText 由 StreamingBubble 自订阅，
  // 避免每个 chunk 重渲染整个面板（含设置页）
  const hasStreamingText = useSessionStore((s) => s.streamingText.length > 0);
  const messages = useSessionStore((s) => s.messages);
  const pendingAttachments = useSessionStore((s) => s.pendingAttachments);
  const lightboxImage = useSessionStore((s) => s.lightboxImage);
  const conversations = useSessionStore((s) => s.conversations);
  const currentConversationId = useSessionStore((s) => s.currentConversationId);
  const currentConversationTitle =
    conversations.find((conversation) => conversation.id === currentConversationId)?.title;

  // ── 侧边栏状态 ──
  const [sidebarOpen, setSidebarOpen] = useState(false);

  // 搜索结果跳转后需要定位高亮的消息 ID（2 秒后自动清除）
  const [highlightMessageId, setHighlightMessageId] = useState<string | null>(null);

  // 高亮 2.5 秒后自动清除
  useEffect(() => {
    if (!highlightMessageId) return;
    const timer = setTimeout(() => setHighlightMessageId(null), 2500);
    return () => clearTimeout(timer);
  }, [highlightMessageId]);

  // ── hooks ──
  const { thinkingPhrase, completionPhrase, setCompletionPhrase, COMPLETION_PHRASES } =
    useThinkingAnimation(isAiResponding, hasStreamingText);
  const {
    handleSend,
    clearConversation,
    handleHintClick,
  } = useChatActions();

  // 启动：优先恢复最近的非空会话（上次对话延续）；没有则新建干净对话
  useEffect(() => {
    void (async () => {
      const conversations = await tauriApi.listConversations().catch(() => []);
      const lastActive = conversations.find((c) => c.message_count > 0);
      if (lastActive) {
        await switchToConversation(lastActive.id);
      } else {
        const conversation = await createAndSwitchConversation();
        if (!conversation) toast.error("新建对话失败，请重新打开助手");
      }
      await refreshConversationList();
    })();
  }, []);

  // ── Tauri 事件监听：AI 流式响应 ──
  useTauriEvent<AiStreamChunkEvent>("ai-stream-chunk", (payload) => {
    if (payload.request_id !== useSessionStore.getState().activeAiRequestId) return;
    useSessionStore.getState().appendStreamingText(payload.chunk);
  });

  useTauriEvent<AiStreamDoneEvent>("ai-stream-done", (payload) => {
    if (payload.request_id !== useSessionStore.getState().activeAiRequestId) return;
    useSessionStore.getState().commitStreamToMessage();
    // 持久化助手消息
    const msgs = useSessionStore.getState().messages;
    const lastMsg = msgs[msgs.length - 1];
    if (lastMsg && lastMsg.role === "assistant") {
      const conversationId = useSessionStore.getState().currentConversationId;
      void persistMessage(lastMsg, conversationId).then((savedMessage) => {
        if (savedMessage) {
          useSessionStore.getState().replaceMessage(savedMessage);
        }
        return refreshConversationList();
      });
    }
    if (useUiStore.getState().isPanelOpen) {
      const phrase = COMPLETION_PHRASES[Math.floor(Math.random() * COMPLETION_PHRASES.length)];
      setCompletionPhrase(phrase);
      setTimeout(() => setCompletionPhrase(""), 2500);
    }
  });

  // ── 图片点击 → 灯箱 ──
  const handleImageClick = useCallback((img: ImageAttachment) => {
    useSessionStore.getState().openLightbox(img);
  }, []);

  // ── 消息撤回 ──
  const handleRetract = async (msgId: string) => {
    if (isAiResponding) {
      toast.error("AI 正在响应中，请稍后再试");
      return;
    }
    const sess = useSessionStore.getState();
    const savedMessages = [...sess.messages];

    try {
      const remaining = await tauriApi.retractMessage(msgId);
      sess.restoreMessages(remaining);
      refreshConversationList();
      toast.success("已撤回", {
        action: {
          label: "撤销",
          onClick: async () => {
            // 撤销：清除 DB → 重新持久化所有消息 → 重建上下文
            try {
              await tauriApi.clearHistoryMessages();
              for (const msg of savedMessages) {
                await tauriApi.saveMessage(msg);
              }
              // 通过 switchConversation 重建 Agent 上下文
              const msgs = await tauriApi.switchConversation(
                useSessionStore.getState().currentConversationId,
              );
              useSessionStore.getState().restoreMessages(msgs);
              refreshConversationList();
              toast.success("已恢复");
            } catch (err) {
              console.warn("[ChatPanel] 撤销撤回失败:", err);
              toast.error("恢复失败");
            }
          },
        },
      });
    } catch (err) {
      console.warn("[ChatPanel] 撤回失败:", err);
      toast.error("撤回失败，请稍后再试");
    }
  };

  // ── 对话操作 ──
  const handleSelectConversation = async (id: string) => {
    if (isAiResponding) {
      toast.error("请等待当前回复完成后再切换对话");
      return;
    }
    if (pendingAttachments.length > 0) {
      toast.error("请先发送或移除待发送附件");
      return;
    }
    if (id === currentConversationId) {
      setSidebarOpen(false);
      return;
    }
    await switchToConversation(id);
    setSidebarOpen(false);
  };

  /** 点击搜索结果：切换会话并定位高亮到匹配的消息 */
  const handleSelectSearchResult = async (convId: string, messageId: string) => {
    if (isAiResponding) {
      toast.error("请等待当前回复完成后再切换对话");
      return;
    }
    if (pendingAttachments.length > 0) {
      toast.error("请先发送或移除待发送附件");
      return;
    }
    if (convId !== currentConversationId) {
      await switchToConversation(convId);
    }
    setSidebarOpen(false);
    setHighlightMessageId(messageId);
  };

  const handleDeleteConversation = async (id: string) => {
    if (isAiResponding) {
      toast.error("请等待当前回复完成后再删除对话");
      return;
    }
    if (id === currentConversationId && pendingAttachments.length > 0) {
      toast.error("请先移除当前对话中的待发送附件");
      return;
    }
    await deleteConversationAndHandle(id);
  };

  const handleRenameConversation = async (id: string, newTitle: string) => {
    await renameConversation(id, newTitle);
  };

  const handleNewConversation = async () => {
    if (isAiResponding) {
      toast.error("请等待当前回复完成后再新建对话");
      return;
    }
    if (pendingAttachments.length > 0) {
      toast.error("请先发送或移除待发送附件");
      return;
    }
    await createAndSwitchConversation();
  };

  // ── 停止生成 ──
  const handleStop = useCallback(() => {
    void tauriApi.stopGeneration().catch((err) => {
      console.warn("[ChatPanel] 停止生成失败:", err);
    });
  }, []);

  // ── 截图提问：后端隐藏本窗口抓屏后打开独立全屏截图窗口 ──
  const handleCapture = useCallback(() => {
    void tauriApi.captureScreen().catch((err) => {
      console.warn("[ChatPanel] 截图失败:", err);
      toast.error("截图失败，请重试");
    });
  }, []);

  // 截图窗口提交的裁剪附件 → 加入待发送区（与粘贴图片同一入口）
  useTauriEvent<ScreenshotAttachmentDto>("screenshot-captured", (payload) => {
    useSessionStore.getState().addPendingAttachment({
      type: "image",
      id: crypto.randomUUID(),
      data: payload.data,
      mimeType: payload.mimeType,
      name: payload.name,
      size: Math.round(payload.data.length * 0.75),
      width: payload.width || undefined,
      height: payload.height || undefined,
      source: "screenshot",
    });
  });

  // ── 渲染 ──
  return (
        <motion.div
          className="absolute inset-0 flex flex-col overflow-hidden"
          style={{
            ...PANEL_STYLE,
            background: hexToRgba(config.bg_color, config.bg_opacity),
            backdropFilter: "blur(20px)",
            WebkitBackdropFilter: "blur(20px)",
            boxShadow: "0 0 0 1px rgba(255,255,255,0.04), 0 2px 24px rgba(0,0,0,0.12)",
          }}
          initial={false}
          animate={{ opacity: isPanelOpen ? 1 : 0, y: isPanelOpen ? 0 : -8 }}
          transition={{ type: "spring", stiffness: 300, damping: 28 }}
          aria-hidden={!isPanelOpen}
        >
          {showSettings ? (
            <SettingsPanel onClose={() => useUiStore.getState().setShowSettings(false)} />
          ) : (
            <>
              {/* 顶部微光装饰 */}
              <div
                className="absolute top-0 left-0 right-0 h-32 pointer-events-none z-0"
                style={{
                  background:
                    "radial-gradient(ellipse 60% 50% at 50% 0%, rgba(129, 140, 248, 0.04) 0%, transparent 100%)",
                }}
                aria-hidden="true"
              />

              {/* 顶部工具栏 */}
              <HeaderToolbar
                isAiResponding={isAiResponding}
                conversationTitle={currentConversationTitle}
                onClear={clearConversation}
                onSettings={() => useUiStore.getState().setShowSettings(true)}
                onCollapse={() => useUiStore.getState().togglePanel()}
                onToggleConversations={() => setSidebarOpen((v) => !v)}
                onCapture={handleCapture}
              />

              {/* 消息列表 */}
              <MessageList
                messages={messages}
                isAiResponding={isAiResponding}
                thinkingPhrase={thinkingPhrase}
                completionPhrase={completionPhrase}
                onHintClick={handleHintClick}
                onImageClick={handleImageClick}
                onRetract={handleRetract}
                highlightMessageId={highlightMessageId}
              />

              {/* 输入区域 */}
              <InputArea
                conversationId={currentConversationId}
                isDisabled={isAiResponding}
                onSend={handleSend}
                onStop={handleStop}
                pendingAttachments={pendingAttachments}
                onAddAttachment={(att) => useSessionStore.getState().addPendingAttachment(att)}
                onRemoveAttachment={(id) => useSessionStore.getState().removePendingAttachment(id)}
              />

              {/* 灯箱 */}
              <Lightbox
                image={lightboxImage}
                onClose={() => useSessionStore.getState().closeLightbox()}
              />

              {/* 对话列表侧边栏 */}
              {sidebarOpen && (
                <ConversationList
                  conversations={conversations}
                  currentId={currentConversationId}
                  onSelect={handleSelectConversation}
                  onSelectMessage={handleSelectSearchResult}
                  onDelete={handleDeleteConversation}
                  onRename={handleRenameConversation}
                  onCreateNew={handleNewConversation}
                  onClose={() => setSidebarOpen(false)}
                  isBusy={isAiResponding}
                />
              )}
            </>
          )}
        </motion.div>
  );
}
