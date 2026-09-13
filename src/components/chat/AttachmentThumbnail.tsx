import { memo, useCallback } from "react";
import type { Attachment } from "../../types";
import { formatFileSize } from "../../lib/formatSize";

interface AttachmentThumbnailProps {
  attachment: Attachment;
  /** "small" = 输入框附件卡片，"normal" = 消息气泡附件卡片 */
  size?: "small" | "normal";
  /** 图片点击回调（仅图片附件触发） */
  onImageClick?: (att: Attachment) => void;
  /** 删除回调（不传则不显示删除按钮） */
  onRemove?: (id: string) => void;
}

/** 附件卡片右上/行尾的删除按钮（图片与文件卡片共用同一外观） */
function RemoveButton({ onClick }: { onClick: (e: React.MouseEvent) => void }) {
  return (
    <button
      className="attachment-remove-btn ml-auto flex-shrink-0"
      onClick={onClick}
      aria-label="删除附件"
      title="删除附件"
    >
      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={3}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  );
}

/**
 * 附件摘要文案：图片取 MIME 子类型，文件取扩展名，统一追加体积。
 *
 * 调用点已按 `attachment.type` 分派，因此这里只需处理对应分支。
 */
function formatAttachmentDetail(attachment: Attachment): string {
  const kind =
    attachment.type === "image"
      ? attachment.mimeType.split("/")[1]?.replace("svg+xml", "svg").toUpperCase()
      : attachment.name.split(".").pop()?.toUpperCase();
  return `${kind || (attachment.type === "image" ? "图片" : "文件")} · ${formatFileSize(attachment.size)}`;
}

/** 附件缩略图 — 共用组件，在输入框预览和消息气泡中复用 */
export const AttachmentThumbnail = memo(function AttachmentThumbnail({
  attachment,
  size = "normal",
  onImageClick,
  onRemove,
}: AttachmentThumbnailProps) {
  const isSmall = size === "small";

  const handleClick = useCallback(() => {
    if (attachment.type === "image") {
      onImageClick?.(attachment);
    }
  }, [attachment, onImageClick]);

  const handleRemove = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      onRemove?.(attachment.id);
    },
    [attachment.id, onRemove],
  );

  if (attachment.type === "image") {
    const imageUrl = attachment.url || `data:${attachment.mimeType};base64,${attachment.data}`;

    return (
      <div
        className={`file-attachment-card image-attachment-card flex-shrink-0${isSmall ? " file-attachment-card-small" : " file-attachment-card-message"}`}
        style={{ maxWidth: isSmall ? 180 : 220 }}
        role={onImageClick ? "button" : undefined}
        tabIndex={onImageClick ? 0 : undefined}
        title="点击预览图片"
        onClick={handleClick}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            handleClick();
          }
        }}
      >
        <div className="image-attachment-preview" aria-hidden="true">
          <img src={imageUrl} alt="" loading="lazy" />
        </div>
        <div className="flex flex-col min-w-0">
          <span className="file-attachment-name" title={attachment.name}>
            {attachment.name || "图片附件"}
          </span>
          <span className="file-attachment-size">{formatAttachmentDetail(attachment)}</span>
        </div>
        {onRemove && <RemoveButton onClick={handleRemove} />}
      </div>
    );
  }

  // 文件附件
  return (
    <div
      className={`file-attachment-card flex-shrink-0${isSmall ? " file-attachment-card-small" : " file-attachment-card-message"}`}
      style={{ maxWidth: isSmall ? 180 : 220 }}
    >
      {/* 文件图标 */}
      <svg
        className="file-attachment-icon"
        width="28"
        height="28"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{ color: "var(--text-tertiary)" }}
      >
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
        <line x1="16" y1="13" x2="8" y2="13" />
        <line x1="16" y1="17" x2="8" y2="17" />
      </svg>
      <div className="flex flex-col min-w-0">
        <span className="file-attachment-name" title={attachment.name}>
          {attachment.name}
        </span>
        <span className="file-attachment-size">{formatAttachmentDetail(attachment)}</span>
      </div>
      {onRemove && <RemoveButton onClick={handleRemove} />}
    </div>
  );
});
