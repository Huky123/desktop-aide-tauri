import { useDeferredValue } from "react";
import { useSessionStore } from "../../store/sessionStore";
import { Markdown } from "./Markdown";
import type { ImageAttachment } from "../../types";

interface StreamingBubbleProps {
  onImageClick?: (img: ImageAttachment) => void;
}

/**
 * 流式回复气泡 —— **自订阅** `streamingText`。
 *
 * 关键：把 streamingText 的订阅收敛到这个叶子组件，父级 ChatPanel / MessageList
 * 不再随每个 chunk 重渲染。此前订阅在面板根，每来一个 chunk 都会重渲染整棵面板树
 * （设置页打开时还包括 900+ 行的 SettingsPanel），长回答掉帧明显。
 *
 * `useDeferredValue` 保留在组件内部：Markdown 重解析降优先级，输入框保持跟手，
 * 滞后期间用半透明表示"还在追"。
 */
export function StreamingBubble({ onImageClick }: StreamingBubbleProps) {
  const streamingText = useSessionStore((s) => s.streamingText);
  const deferredStreamingText = useDeferredValue(streamingText);

  if (!streamingText) return null;

  return (
    <div className="flex justify-start">
      <div
        className="message-bubble message-assistant max-w-[86%] px-3 py-2 text-[13px] leading-relaxed select-text"
        style={{
          color: "var(--assistant-text)",
          opacity: streamingText !== deferredStreamingText ? 0.7 : 1,
        }}
      >
        <Markdown content={deferredStreamingText} onImageClick={onImageClick} />
        <span className="streaming-cursor" />
      </div>
    </div>
  );
}
