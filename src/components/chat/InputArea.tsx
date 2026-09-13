import { useState, useRef, useCallback, useEffect, useLayoutEffect, memo } from "react";
import { toast } from "sonner";
import type { Attachment, FileAttachment } from "../../types";
import { AttachmentThumbnail } from "./AttachmentThumbnail";
import { resizeImageToBase64 } from "../../lib/imageResize";

interface InputAreaProps {
  conversationId: string;
  isDisabled: boolean;
  onSend: (text: string, attachments: Attachment[]) => void;
  /** AI 回复中点击"停止" */
  onStop: () => void;
  /** 待发送附件列表 */
  pendingAttachments: Attachment[];
  onAddAttachment: (att: Attachment) => void;
  onRemoveAttachment: (id: string) => void;
}

const MAX_TEXT_FILE_BYTES = 512 * 1024;
const MAX_TEXT_FILE_CHARS = 50_000;
const TEXT_FILE_EXTENSIONS = new Set([
  "txt", "md", "csv", "json", "xml", "yaml", "yml", "log",
  "js", "jsx", "ts", "tsx", "css", "html", "py", "rs", "java", "go", "sql",
]);

function isSupportedTextFile(file: File) {
  const extension = file.name.split(".").pop()?.toLowerCase() || "";
  return file.type.startsWith("text/")
    || file.type === "application/json"
    || file.type === "application/xml"
    || TEXT_FILE_EXTENSIONS.has(extension);
}

/** 输入框 + 附件预览 + 文件/发送按钮 */
export const InputArea = memo(function InputArea({
  conversationId,
  isDisabled,
  onSend,
  onStop,
  pendingAttachments,
  onAddAttachment,
  onRemoveAttachment,
}: InputAreaProps) {
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const input = drafts[conversationId] || "";
  const setInput = useCallback((value: string) => {
    setDrafts((current) => ({ ...current, [conversationId]: value }));
  }, [conversationId]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const MAX_HEIGHT_PX = 88;

  // 自动调整 textarea 高度
  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT_PX)}px`;
  }, [input]);

  // 监听 hint-fill-input / hint-focus-input
  useEffect(() => {
    const handleFill = (e: Event) => {
      const text = (e as CustomEvent).detail as string;
      if (text) {
        setInput(text);
        textareaRef.current?.focus();
      }
    };
    const handleFocus = () => {
      textareaRef.current?.focus();
    };
    window.addEventListener("hint-fill-input", handleFill);
    window.addEventListener("hint-focus-input", handleFocus);
    return () => {
      window.removeEventListener("hint-fill-input", handleFill);
      window.removeEventListener("hint-focus-input", handleFocus);
    };
  }, [setInput]);

  const addFile = useCallback(
    async (file: File, source: "clipboard" | "file") => {
      try {
        if (file.type.startsWith("image/")) {
          const { data, mimeType, width, height } = await resizeImageToBase64(file);
          onAddAttachment({
            type: "image",
            id: crypto.randomUUID(),
            data,
            mimeType,
            name: file.name || `图片.${mimeType.split("/")[1] || "png"}`,
            size: Math.round(data.length * 0.75),
            width,
            height,
            source,
          });
          return;
        }

        if (!isSupportedTextFile(file)) {
          toast.error("暂不支持该文件格式，请选择图片、文本或代码文件");
          return;
        }
        if (file.size > MAX_TEXT_FILE_BYTES) {
          toast.error("文本文件不能超过 512 KB");
          return;
        }

        const fullText = await file.text();
        const truncated = fullText.length > MAX_TEXT_FILE_CHARS;
        const attachment: FileAttachment = {
          type: "file",
          id: crypto.randomUUID(),
          name: file.name || "文本文件.txt",
          size: file.size,
          mimeType: file.type || "text/plain",
          data: fullText.slice(0, MAX_TEXT_FILE_CHARS),
          truncated,
        };
        onAddAttachment(attachment);
        if (truncated) toast.warning("文件较长，仅发送前 50,000 个字符");
      } catch (error) {
        console.error("读取附件失败:", error);
        toast.error("读取附件失败");
      }
    },
    [onAddAttachment],
  );

  const handleFileSelection = useCallback(
    (files: FileList | null) => {
      if (!files) return;
      Array.from(files).forEach((file) => void addFile(file, "file"));
    },
    [addFile],
  );

  // ── 粘贴处理：图像 / 文件 ──
  const handlePaste = useCallback(
    (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const items = e.clipboardData?.items;
      if (!items) return;

      for (let i = 0; i < items.length; i++) {
        const item = items[i];

        // 图片粘贴
        if (item.type.startsWith("image/")) {
          e.preventDefault();
          const blob = item.getAsFile();
          if (!blob) continue;

          void addFile(blob, "clipboard");
          continue;
        }

        // 文件粘贴
        if (item.kind === "file") {
          e.preventDefault();
          const file = item.getAsFile();
          if (!file) continue;

          void addFile(file, "clipboard");
        }
        // text/plain 不拦截，让文字正常粘贴到 textarea
      }
    },
    [addFile],
  );

  // ── 拖拽文件支持（拖入图片/文本文件 → 附件） ──
  const [dragging, setDragging] = useState(false);
  const dragDepthRef = useRef(0);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }, []);

  const handleDragEnter = useCallback(() => {
    dragDepthRef.current += 1;
    setDragging(true);
  }, []);

  const handleDragLeave = useCallback(() => {
    dragDepthRef.current -= 1;
    if (dragDepthRef.current <= 0) {
      dragDepthRef.current = 0;
      setDragging(false);
    }
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      dragDepthRef.current = 0;
      setDragging(false);
      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;
      Array.from(files).forEach((file) => void addFile(file, "file"));
    },
    [addFile],
  );

  // ── 发送 ──
  const handleSend = useCallback(() => {
    const text = textareaRef.current?.value.trim() || "";
    if ((!text && pendingAttachments.length === 0) || isDisabled) return;
    onSend(text, pendingAttachments);
    setInput("");
  }, [isDisabled, onSend, pendingAttachments, setInput]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      const text = e.currentTarget.value.trim() || "";
      if ((!text && pendingAttachments.length === 0) || isDisabled) return;
      onSend(text, pendingAttachments);
      setInput("");
    }
  };

  const hasContent = input.trim().length > 0 || pendingAttachments.length > 0;
  const canSend = hasContent && !isDisabled;
  const placeholder = isDisabled ? "AI 正在回复..." : "输入消息，或粘贴图片/文件...";

  return (
    <div
      className="input-shell px-2.5 py-2"
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      style={
        dragging
          ? { boxShadow: "inset 0 0 0 2px var(--accent)" }
          : undefined
      }
    >
      {/* 附件预览条 */}
      {pendingAttachments.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mb-2">
          {pendingAttachments.map((att) => (
            <AttachmentThumbnail
              key={att.id}
              attachment={att}
              size="small"
              onRemove={(id) => onRemoveAttachment(id)}
            />
          ))}
        </div>
      )}

      <div className="flex items-end gap-2">
        <input
          ref={fileInputRef}
          type="file"
          multiple
          className="sr-only"
          accept="image/*,.txt,.md,.csv,.json,.xml,.yaml,.yml,.log,.js,.jsx,.ts,.tsx,.css,.html,.py,.rs,.java,.go,.sql"
          onChange={(event) => {
            handleFileSelection(event.target.files);
            event.target.value = "";
          }}
        />
        <button
          type="button"
          onClick={() => fileInputRef.current?.click()}
          disabled={isDisabled}
          className="btn-icon flex-shrink-0"
          title="添加图片或文本文件"
          aria-label="添加图片或文本文件"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" style={{ color: "var(--text-tertiary)" }}>
            <path d="m21.4 11.6-8.9 8.9a6 6 0 0 1-8.5-8.5l9.6-9.6a4 4 0 0 1 5.7 5.7l-9.6 9.6a2 2 0 0 1-2.8-2.8l8.9-8.9" />
          </svg>
        </button>

        <textarea
          ref={textareaRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder={placeholder}
          className="input-field flex-1 text-[13px] resize-none px-3 py-1.5"
          style={{
            minHeight: 30,
            maxHeight: MAX_HEIGHT_PX,
          }}
          rows={1}
        />
        {isDisabled ? (
          <button
            onClick={onStop}
            aria-label="停止生成"
            title="停止生成"
            className="btn-send"
            style={{ background: "var(--surface-active)", cursor: "pointer" }}
          >
            <svg
              className="w-3 h-3"
              viewBox="0 0 24 24"
              fill="currentColor"
              style={{ color: "var(--text-secondary)" }}
            >
              <rect x="6" y="6" width="12" height="12" rx="2" />
            </svg>
          </button>
        ) : (
          <button
            onClick={handleSend}
            disabled={!canSend}
            aria-label="发送消息"
            className="btn-send"
            style={{
              background: canSend ? "var(--accent)" : "var(--surface-active)",
              cursor: canSend ? "pointer" : "not-allowed",
            }}
          >
            <svg
              className="w-3.5 h-3.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
              style={{ color: canSend ? "white" : "var(--text-tertiary)" }}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
              />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
});
