import { useCallback } from "react";
import { toast } from "sonner";
import { useSessionStore } from "../store/sessionStore";
import { tauriApi } from "../services/tauriApi";
import { classifyAiError } from "../lib/errorClassifier";
import { persistMessage, clearHistoryDb } from "../services/chat/messagePersistence";
import { refreshConversationList } from "../services/chat/conversationService";
import type { Attachment, ImageAttachment, FileAttachment, AttachmentDto } from "../types";

/** 将前端 Attachment 列表转换为后端 AttachmentDto 列表（仅图片） */
function toImageDtos(attachments: Attachment[]): AttachmentDto[] {
  return attachments
    .filter((a): a is ImageAttachment => a.type === "image")
    .map((a) => ({
      data: a.data,
      mime_type: a.mimeType,
      name: a.name,
    }));
}

/** 构建包含附件信息的消息文本 */
function buildMessageWithAttachments(text: string, attachments: Attachment[]): string {
  if (attachments.length === 0) return text;

  const imageNames = attachments
    .filter((a) => a.type === "image")
    .map((a) => a.name);
  const fileNames = attachments
    .filter((a) => a.type === "file")
    .map((a) => a.name);

  const parts: string[] = [];
  if (text.trim()) parts.push(text);
  if (imageNames.length > 0) {
    parts.push(imageNames.length === 1 ? `[图片: ${imageNames[0]}]` : `[图片: ${imageNames.join(", ")}]`);
  }
  if (fileNames.length > 0) {
    parts.push(fileNames.length === 1 ? `[文件: ${fileNames[0]}]` : `[文件: ${fileNames.join(", ")}]`);
  }

  return parts.join("\n") || "请分析以下附件";
}

/** 将文本文件内容加入模型输入，但不把大段正文显示在用户消息气泡中。 */
function buildModelInput(text: string, attachments: Attachment[]): string {
  const displayText = buildMessageWithAttachments(text, attachments);
  const fileSections = attachments
    .filter((attachment): attachment is FileAttachment => attachment.type === "file" && !!attachment.data)
    .map((attachment) => {
      const truncationNote = attachment.truncated ? "\n[内容已截断]" : "";
      return `<file name="${attachment.name}">\n${attachment.data}${truncationNote}\n</file>`;
    });
  return fileSections.length > 0
    ? `${displayText}\n\n${fileSections.join("\n\n")}`
    : displayText;
}

/** 统一的 AI 错误处理 — handleSend 共享 */
function handleAiError(err: unknown, requestId: string) {
  console.error("AI 请求失败:", err);
  const s = useSessionStore.getState();
  if (s.activeAiRequestId !== requestId) {
    console.warn("[handleAiError] 忽略过期请求的错误", { requestId, activeId: s.activeAiRequestId });
    return;
  }
  const classified = classifyAiError(err);
  const conversationId = s.currentConversationId;
  const messageCountBefore = s.messages.length;
  s.commitStreamToMessage(
    undefined,
    "\n\n> 回复因连接中断，以上内容可能不完整。",
  );

  // 已收到部分内容时优先保留，不再用通用错误消息覆盖有效回答。
  const latestState = useSessionStore.getState();
  if (latestState.messages.length > messageCountBefore) {
    const partialMessage = latestState.messages[latestState.messages.length - 1];
    void persistMessage(partialMessage, conversationId).then(refreshConversationList);
    toast.warning("回复中断，已保留生成内容", { id: `ai-error-${requestId}` });
    return;
  }

  toast.error(classified.title, { id: `ai-error-${requestId}` });
  const errorMsg = {
    id: crypto.randomUUID(),
    role: "assistant" as const,
    content: `❌ ${classified.title}。${classified.detail}`,
    timestamp: Date.now(),
  };
  s.addMessage(errorMsg);
  persistMessage(errorMsg, conversationId);
  s.setAiResponding(false);
  s.setActiveAiRequestId(null);
  s.clearStreamingText();
}

/** 统一的 AI 请求初始化 — 生成 requestId 并设置状态 */
function startAiRequest() {
  const requestId = crypto.randomUUID();
  const sess = useSessionStore.getState();
  sess.setActiveAiRequestId(requestId);
  sess.setAiResponding(true);
  sess.clearStreamingText();
  return requestId;
}

/**
 * 集中封装 ChatPanel 中所有业务逻辑操作。
 *
 * 设计决策：所有 store 读取均在回调内通过 `getState()` 完成（而非 hook 订阅），
 * 避免因 store 值变化导致 useCallback 引用漂移，同时减少父组件不必要的重渲染。
 */
export function useChatActions() {
  // ── 清除对话（toast + 撤销）──
  const clearConversation = useCallback(() => {
    const sess = useSessionStore.getState();
    const conversationId = sess.currentConversationId;
    const savedMessages = [...sess.messages];
    sess.clearMessages();
    sess.clearStreamingText();
    tauriApi.resetConversation().catch(() => {});
    void clearHistoryDb(conversationId);
    // 刷新对话列表（更新预览）
    refreshConversationList();
    toast.success("对话已清除", {
      action: {
        label: "撤销",
        onClick: async () => {
          if (useSessionStore.getState().currentConversationId !== conversationId) {
            toast.error("已切换对话，无法在当前对话撤销");
            return;
          }
          try {
            // 1. 恢复数据库记录
            await Promise.all(
              savedMessages.map((message) => persistMessage(message, conversationId)),
            );
            // 2. 通过 switchConversation 重建 Agent 上下文（与撤回撤销行为一致）
            const messages = await tauriApi.switchConversation(conversationId);
            useSessionStore.getState().restoreMessages(messages);
            refreshConversationList();
            toast.success("已恢复对话");
          } catch (err) {
            console.warn("[useChatActions] 撤销清除对话失败:", err);
            toast.error("恢复失败，请稍后再试");
          }
        },
      },
    });
  }, []);

  // ── 斜杠命令 ──
  const handleSlashCommand = useCallback(
    (cmd: string): boolean => {
      if (cmd === "/clear") {
        clearConversation();
        return true;
      }
      if (cmd === "/copy") {
        const msgs = useSessionStore.getState().messages;
        const lastAssistant = [...msgs].reverse().find((m) => m.role === "assistant");
        if (lastAssistant) {
          navigator.clipboard.writeText(lastAssistant.content).then(
            () => toast.success("已复制最后一条回复"),
            () => toast.error("复制失败"),
          );
        } else {
          toast.error("没有可复制的内容");
        }
        return true;
      }
      if (cmd === "/help") {
        const helpMsg = {
          id: crypto.randomUUID(),
          role: "assistant" as const,
          content:
            "**可用命令：**\n\n" +
            "- `/clear` — 清除对话历史\n" +
            "- `/copy` — 复制最后一条 AI 回复到剪贴板\n" +
            "- `/remind 10分钟 内容` — 设置定时提醒（支持 秒/分钟/小时/天 或 14:30）\n" +
            "- `/help` — 显示此帮助信息\n\n" +
            "**快捷键：**\n" +
            "- `Enter` 发送消息\n" +
            "- `Shift + Enter` 换行\n" +
            "- `Ctrl + V` 粘贴图片/文件\n" +
            "- `Ctrl + Alt + Space` 切换面板\n" +
            "- `Ctrl + Alt + C` 截图提问\n" +
            "- `Esc` 收起面板",
          timestamp: Date.now(),
        };
        useSessionStore.getState().addMessage(helpMsg);
        persistMessage(helpMsg);
        return true;
      }
      if (cmd.startsWith("/remind")) {
        const parts = cmd.split(/\s+/);
        const timeExpr = parts[1] || "";
        const content = parts.slice(2).join(" ") || "提醒";
        if (!timeExpr) {
          toast.error("用法: /remind 10分钟 提醒我喝水");
          return true;
        }
        void tauriApi
          .createReminder(timeExpr, content)
          .then((reminder) => {
            const when = new Date(reminder.remind_at).toLocaleTimeString("zh-CN", {
              hour: "2-digit",
              minute: "2-digit",
            });
            toast.success(`已设置提醒：今天 ${when} ${reminder.content}`);
          })
          .catch((err) => {
            console.warn("[useChatActions] 创建提醒失败:", err);
            toast.error(`提醒创建失败: ${String(err)}`);
          });
        return true;
      }
      return false;
    },
    [clearConversation],
  );

  // ── 发送消息 ──
  const handleSend = useCallback(
    async (text: string, attachments: Attachment[] = []) => {
      // 仅文本 + 无附件时检查斜杠命令
      if (attachments.length === 0 && text.startsWith("/") && handleSlashCommand(text)) return;

      const requestId = startAiRequest();
      const sess = useSessionStore.getState();

      const displayText = buildMessageWithAttachments(text, attachments);
      const modelInput = buildModelInput(text, attachments);
      const conversationId = sess.currentConversationId;
      const isFirstMessage = sess.messages.length === 0;

      const userMsg = {
        id: crypto.randomUUID(),
        role: "user" as const,
        content: displayText,
        timestamp: Date.now(),
        attachments: attachments.length > 0 ? [...attachments] : undefined,
      };
      sess.addMessage(userMsg);
      void persistMessage(userMsg, conversationId).then(refreshConversationList);

      // 首条消息：异步生成 AI 会话标题（失败静默，不影响发送）
      if (isFirstMessage) {
        void tauriApi
          .generateConversationTitle(conversationId)
          .then((title) => {
            if (title) {
              useSessionStore.getState().updateConversation(conversationId, { title });
              refreshConversationList();
            }
          })
          .catch(() => {});
      }

      // 发送后清除待发送附件
      sess.clearPendingAttachments();

      try {
        const imageDtos = toImageDtos(attachments);
        await tauriApi.aiChat(
          requestId,
          modelInput,
          imageDtos.length > 0 ? imageDtos : undefined,
        );
      } catch (err) {
        handleAiError(err, requestId);
      }
    },
    [handleSlashCommand],
  );

  // ── 欢迎标签点击 — 填入预设文案或聚焦输入框 ──
  const handleHintClick = useCallback(
    (action: string) => {
      if (action === "提问") {
        window.dispatchEvent(new CustomEvent("hint-focus-input"));
        return;
      }
      if (action === "翻译") {
        window.dispatchEvent(
          new CustomEvent("hint-fill-input", { detail: "请帮我翻译以下内容：" }),
        );
        return;
      }
      if (action === "解释") {
        window.dispatchEvent(
          new CustomEvent("hint-fill-input", { detail: "请解释一下：" }),
        );
        return;
      }
      if (action === "识别图片") {
        window.dispatchEvent(
          new CustomEvent("hint-fill-input", { detail: "请帮我分析以下图片" }),
        );
        return;
      }
    },
    [],
  );

  return {
    handleSend,
    clearConversation,
    handleHintClick,
  } as const;
}
