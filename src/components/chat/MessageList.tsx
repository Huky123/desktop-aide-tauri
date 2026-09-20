import { useRef, useEffect, useCallback, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { MessageBubble } from "./MessageBubble";
import { StreamingBubble } from "./StreamingBubble";
import { ReminderStack } from "./ReminderStack";
import { useReminderStore } from "../../store/reminderStore";
import { useSessionStore } from "../../store/sessionStore";
import { useConfigStore } from "../../store/configStore";
import { useUiStore } from "../../store/uiStore";
import type { ChatMessage, ImageAttachment } from "../../types";

/** 判定用户是否在滚动容器底部的阈值（像素） */
const SCROLL_BOTTOM_THRESHOLD = 64;

interface MessageListProps {
  messages: ChatMessage[];
  isAiResponding: boolean;
  thinkingPhrase: string;
  completionPhrase: string;
  onHintClick?: (action: string) => void;
  onImageClick?: (img: ImageAttachment) => void;
  onRetract?: (msgId: string) => void;
  /** 搜索结果跳转后需要滚动定位并高亮的消息 ID */
  highlightMessageId?: string | null;
  /** 当前会话 ID：切换会话时重置滚动跟随状态 */
  sessionKey?: string;
}

/**
 * 未配置 AI 服务时的引导卡。
 *
 * 背景：装完即用并不成立——没有 API Key 一个请求也发不出去，而界面原本对此毫无
 * 提示，用户只能自己猜"为什么发了消息没反应"。设置里现在有「测试连接」，
 * 所以这里直接把人送过去，而不是让他自己试错。
 */
function SetupCard() {
  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: "easeOut" }}
      className="empty-state mt-6 flex flex-col items-center justify-center"
    >
      <div className="welcome-glyph" aria-hidden="true">
        <svg className="w-[22px] h-[22px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.8}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M15 7a4 4 0 1 0-3.9 5H11l-1 1H8l-1 1v2h2l6-6a4 4 0 0 0 0-3Z" />
        </svg>
      </div>
      <p className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        还差一步就能用了
      </p>
      <p
        className="text-xs text-center mt-1"
        style={{ color: "var(--text-secondary)", maxWidth: 250, lineHeight: 1.55 }}
      >
        填上 AI 服务商的 API Key 即可开始对话。设置里有「测试连接」，可以一次性验完
        Key、模型名和端点，不用靠试错。
      </p>
      <div className="welcome-hints welcome-hints--single">
        <button
          className="welcome-hint-btn"
          onClick={() => useUiStore.getState().setShowSettings(true)}
        >
          去设置
        </button>
      </div>
    </motion.div>
  );
}

/** 欢迎卡片 — 无消息时显示 */
function WelcomeCard({ onHintClick }: { onHintClick?: (action: string) => void }) {
  const config = useConfigStore((s) => s.config);

  // Ollama 跑本地模型不需要 Key；其余服务商没填 Key 就一个请求也发不出去
  if (config.ai_provider !== "ollama" && config.api_key.trim() === "") {
    return <SetupCard />;
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: "easeOut" }}
      className="empty-state mt-6 flex flex-col items-center justify-center"
    >
      <div className="welcome-glyph" aria-hidden="true">
        <svg className="w-[22px] h-[22px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.8}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M8 10h8M8 14h5" />
          <path strokeLinecap="round" strokeLinejoin="round" d="M5 5h14a2 2 0 012 2v9a2 2 0 01-2 2h-7l-4 3v-3H5a2 2 0 01-2-2V7a2 2 0 012-2z" />
        </svg>
      </div>
      <p className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        有什么需要处理？
      </p>
      <div className="welcome-hints">
        {["提问", "翻译", "解释", "识别图片"].map((label) => (
          <button
            key={label}
            className="welcome-hint-btn"
            onClick={() => onHintClick?.(label)}
          >
            {label}
          </button>
        ))}
      </div>
    </motion.div>
  );
}

/** 打字指示器 — 三个脉冲圆点 */
function TypingDots() {
  return (
    <span className="typing-dots">
      <span className="typing-dot" />
      <span className="typing-dot" />
      <span className="typing-dot" />
    </span>
  );
}

/** 消息列表 + 流式响应 + 空状态 + 完成反馈 + 思考指示器 */
export function MessageList({
  messages,
  isAiResponding,
  thinkingPhrase,
  completionPhrase,
  onHintClick,
  onImageClick,
  onRetract,
  highlightMessageId,
  sessionKey,
}: MessageListProps) {
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  // 跟踪用户是否上滚离开了底部区域（用于粘性滚动判断）
  const userScrolledUpRef = useRef(false);
  const [showScrollToBottom, setShowScrollToBottom] = useState(false);
  // 只订阅"是否有流式文本"这个布尔值（整轮只会翻转两次），
  // 完整 streamingText 由 StreamingBubble 自行订阅，避免每个 chunk 重渲染本组件
  const hasStreamingText = useSessionStore((s) => s.streamingText.length > 0);
  // 面板内提醒卡片数量（新提醒到来时强制滚动到底部，确保提醒被看到）
  const reminderCount = useReminderStore((s) => s.items.length);

  /** 检查用户是否在底部附近 */
  const isNearBottom = useCallback(() => {
    const el = listRef.current;
    if (!el) return true;
    return el.scrollHeight - el.scrollTop - el.clientHeight < SCROLL_BOTTOM_THRESHOLD;
  }, []);

  // 监听用户手动滚动
  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    const handleScroll = () => {
      userScrolledUpRef.current = !isNearBottom();
      setShowScrollToBottom(userScrolledUpRef.current);
    };
    el.addEventListener("scroll", handleScroll, { passive: true });
    return () => el.removeEventListener("scroll", handleScroll);
  }, [isNearBottom]);

  // 切换会话时重置为「跟随底部」状态（新会话应看到最新消息）
  // 滚到底部后由 scroll 事件自动同步"回到最新"按钮状态，无需手动 setState
  useEffect(() => {
    userScrolledUpRef.current = false;
  }, [sessionKey]);

  // 新消息自动滚动到底部
  useEffect(() => {
    const lastMessage = messages[messages.length - 1];
    const userJustSent = lastMessage?.role === "user";

    // 用户主动发送消息：总是滚到底部（明确意图，即使此前在上滚阅读）
    if (userJustSent) {
      userScrolledUpRef.current = false;
      messagesEndRef.current?.scrollIntoView({ behavior: "instant" });
      return;
    }

    // 流式输出中：用户上滚阅读历史时不打扰
    if (isAiResponding) {
      if (!userScrolledUpRef.current) {
        messagesEndRef.current?.scrollIntoView({ behavior: "instant" });
      }
      return;
    }

    // 输出完成 / 消息变化：仅当用户本来就在底部附近时才跟随，
    // 避免把正在上滚阅读的用户强制拉回末尾
    if (!userScrolledUpRef.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [messages, isAiResponding]);

  // 流式期间跟随文字滚动：直接订阅 store，不经过本组件重渲染
  useEffect(() => {
    const unsubscribe = useSessionStore.subscribe((state, prevState) => {
      if (state.streamingText === prevState.streamingText) return;
      if (userScrolledUpRef.current) return;
      messagesEndRef.current?.scrollIntoView({ behavior: "instant" });
    });
    return unsubscribe;
  }, []);

  // 提醒卡片数量变化（新增/关闭）时滚动到底部，提醒必须被看到
  useEffect(() => {
    if (reminderCount === 0) return;
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [reminderCount]);

  // 搜索结果定位：滚动到匹配消息并高亮（放在自动滚底 effect 之后，确保覆盖它）
  useEffect(() => {
    if (!highlightMessageId) return;
    const container = listRef.current;
    if (!container) return;
    const target = container.querySelector(
      `[data-message-id="${highlightMessageId}"]`,
    );
    if (!target) return;
    // 标记为用户已上滚，避免后续自动滚底把视图拉走
    userScrolledUpRef.current = true;
    target.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [highlightMessageId, messages]);

  const scrollToBottom = useCallback(() => {
    userScrolledUpRef.current = false;
    setShowScrollToBottom(false);
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  return (
    <div className="flex-1 min-h-0 relative">
      <div ref={listRef} className="message-list h-full overflow-y-auto px-2.5 py-2">
      {messages.length === 0 && !isAiResponding && <WelcomeCard onHintClick={onHintClick} />}

      <div className="space-y-3">
        {/* 已有消息 */}
        {messages.map((msg) => (
          <div
            key={msg.id}
            data-message-id={msg.id}
            style={
              msg.id === highlightMessageId
                ? {
                    borderRadius: 10,
                    boxShadow: "0 0 0 2px var(--accent)",
                    background: "rgba(129, 140, 248, 0.08)",
                    transition: "box-shadow 0.3s ease, background 0.3s ease",
                  }
                : undefined
            }
          >
            <MessageBubble
              message={msg}
              onImageClick={onImageClick}
              onRetract={onRetract}
            />
          </div>
        ))}

        {/* 流式响应 — 由 StreamingBubble 自行订阅，避免整列表随 chunk 重渲染 */}
        {isAiResponding && <StreamingBubble onImageClick={onImageClick} />}

        {/* 思考中 */}
        {isAiResponding && !hasStreamingText && (
          <div className="flex justify-start">
            <div
              className="message-bubble message-assistant px-3 py-2 text-[12px] flex items-center gap-2"
              style={{
                color: "var(--text-tertiary)",
              }}
            >
              <TypingDots />
              <span>{thinkingPhrase}</span>
            </div>
          </div>
        )}

        {/* 完成反馈 */}
        <AnimatePresence>
          {!isAiResponding && completionPhrase && (
            <motion.div
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              className="flex justify-start"
            >
              <div
                className="px-3 py-1.5 rounded-lg text-[11px] flex items-center gap-1.5"
                style={{
                  background: "rgba(74, 222, 128, 0.08)",
                  color: "var(--feedback-success)",
                }}
              >
                <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
                </svg>
                {completionPhrase}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

        {/* 面板内提醒卡片（方案 A，内存级，不持久化） */}
        <ReminderStack />

        <div ref={messagesEndRef} />
      </div>
      <AnimatePresence>
        {showScrollToBottom && (
          <motion.button
            type="button"
            className="scroll-to-bottom-btn"
            onClick={scrollToBottom}
            aria-label="回到最新消息"
            title="回到最新消息"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 6 }}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
          </motion.button>
        )}
      </AnimatePresence>
    </div>
  );
}
