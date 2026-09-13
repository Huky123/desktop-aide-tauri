import { memo, useCallback } from "react";
import { Markdown } from "./Markdown";
import { AttachmentThumbnail } from "./AttachmentThumbnail";
import type { Attachment, ChatMessage, ImageAttachment } from "../../types";

interface MessageBubbleProps {
  message: ChatMessage;
  onImageClick?: (img: ImageAttachment) => void;
  onRetract?: (msgId: string) => void;
}

/** 附件缩略图行 — 提取为独立组件以稳定化回调引用，保持 memo 优化有效 */
const AttachmentsRow = memo(function AttachmentsRow({
  attachments,
  onImageClick,
}: {
  attachments: Attachment[];
  onImageClick?: (img: ImageAttachment) => void;
}) {
  return (
    <div className="flex flex-wrap justify-end gap-1.5">
      {attachments.map((att) => (
        <AttachmentThumbnail
          key={att.id}
          attachment={att}
          size="normal"
          onImageClick={
            att.type === "image"
              ? () => onImageClick?.(att as ImageAttachment)
              : undefined
          }
        />
      ))}
    </div>
  );
});

function RetractButton({ onClick, className = "" }: { onClick: () => void; className?: string }) {
  return (
    <button
      className={`msg-retract-btn ${className}`}
      onClick={onClick}
      title="撤回"
      aria-label="撤回消息"
    >
      <svg
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M3 6h18" />
        <path d="M8 6V4h8v4" />
        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
        <path d="M10 11v6" />
        <path d="M14 11v6" />
      </svg>
    </button>
  );
}

/** 隐藏系统为模型上下文补充的附件标签，避免与可视化附件重复。 */
function getVisibleUserContent(content: string, attachments: Attachment[] | undefined) {
  if (!attachments?.length) return content;

  const imageNames = attachments
    .filter((attachment) => attachment.type === "image")
    .map((attachment) => attachment.name)
    .join(", ");
  const fileNames = attachments
    .filter((attachment) => attachment.type === "file")
    .map((attachment) => attachment.name)
    .join(", ");
  const generatedLabels = new Set([
    imageNames && `[图片: ${imageNames}]`,
    fileNames && `[文件: ${fileNames}]`,
  ]);

  return content
    .split("\n")
    .filter((line) => !generatedLabels.has(line.trim()))
    .join("\n")
    .trim();
}

/** 单条消息气泡 — memo 避免流式更新时全部重渲染 */
export const MessageBubble = memo(function MessageBubble({
  message,
  onImageClick,
  onRetract,
}: MessageBubbleProps) {
  const isUser = message.role === "user";
  const hasAttachments =
    message.attachments && message.attachments.length > 0;
  const visibleContent = isUser
    ? getVisibleUserContent(message.content, message.attachments)
    : message.content;
  const showRetract = isUser && onRetract;

  const handleRetract = useCallback(() => {
    onRetract?.(message.id);
  }, [onRetract, message.id]);

  if (isUser) {
    return (
      <div className="message-item flex flex-col items-end gap-1.5">
        {visibleContent && (
          <div
            className={`message-bubble message-user max-w-[86%] px-3 py-2 text-[13px] leading-relaxed select-text relative${showRetract ? " message-has-action" : ""}`}
            style={{
              background: "var(--msg-user-bg)",
              border: "1px solid var(--msg-user-border)",
              color: "var(--text-primary)",
              boxShadow: "0 1px 4px rgba(129, 140, 248, 0.08)",
            }}
          >
            <div className="whitespace-pre-line">{visibleContent}</div>
            {showRetract && <RetractButton onClick={handleRetract} />}
          </div>
        )}
        {hasAttachments && (
          <div className="flex max-w-[86%] items-start gap-1">
            <AttachmentsRow
              attachments={message.attachments!}
              onImageClick={onImageClick}
            />
            {showRetract && !visibleContent && (
              <RetractButton onClick={handleRetract} className="attachment-retract-btn" />
            )}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="message-item flex justify-start">
      <div
        className="message-bubble message-assistant max-w-[86%] px-3 py-2 text-[13px] leading-relaxed select-text relative"
        style={{ color: "var(--assistant-text)" }}
      >
        <Markdown content={message.content} onImageClick={onImageClick} />
      </div>
    </div>
  );
});
