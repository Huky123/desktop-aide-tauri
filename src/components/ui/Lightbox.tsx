import { useState, useRef, useCallback, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { toast } from "sonner";
import { save } from "@tauri-apps/plugin-dialog";
import { tauriApi } from "../../services/tauriApi";
import type { ImageAttachment } from "../../types";

interface LightboxProps {
  image: ImageAttachment | null;
  onClose: () => void;
}

const MIN_SCALE = 1;
const MAX_SCALE = 10;
const ZOOM_STEP = 0.25;

function imageSource(image: ImageAttachment) {
  return image.url || `data:${image.mimeType};base64,${image.data}`;
}

function downloadFilename(image: ImageAttachment) {
  if (/\.[a-z0-9]{2,5}$/i.test(image.name)) return image.name;
  const extension = image.mimeType.split("/")[1]?.replace("svg+xml", "svg") || "png";
  return `${image.name || "AI-图片"}.${extension}`;
}

function triggerDownload(href: string, filename: string) {
  const anchor = document.createElement("a");
  anchor.href = href;
  anchor.download = filename;
  anchor.style.display = "none";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
}

/** 灯箱内部内容 —— 独立组件，通过 key 在图片切换时重置状态 */
function LightboxInner({ image, onClose }: { image: ImageAttachment; onClose: () => void }) {
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef({ x: 0, y: 0 });
  const source = imageSource(image);

  // Escape 关闭
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose]);

  // 滚轮缩放
  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    setScale((prev) => {
      const delta = e.deltaY > 0 ? -ZOOM_STEP : ZOOM_STEP;
      return Math.min(MAX_SCALE, Math.max(MIN_SCALE, prev + delta));
    });
  }, []);

  const zoomBy = useCallback((amount: number) => {
    setScale((previous) => Math.min(MAX_SCALE, Math.max(MIN_SCALE, previous + amount)));
  }, []);

  const resetView = useCallback(() => {
    setScale(1);
    setPan({ x: 0, y: 0 });
  }, []);

  const handleDoubleClick = useCallback(() => {
    setScale((previous) => (previous > 1 ? 1 : 2));
    setPan({ x: 0, y: 0 });
  }, []);

  const handleDownload = useCallback(async () => {
    const filename = downloadFilename(image);
    try {
      const response = await fetch(source);
      if (!response.ok) throw new Error(`下载失败: ${response.status}`);
      const blob = await response.blob();
      const objectUrl = URL.createObjectURL(blob);
      triggerDownload(objectUrl, filename);
      window.setTimeout(() => URL.revokeObjectURL(objectUrl), 0);
    } catch (error) {
      console.warn("图片内容无法读取，改用直接下载:", error);
      triggerDownload(source, filename);
    }
  }, [image, source]);

  // "保存到…"：走系统保存对话框 + 后端写文件。
  // 相比 <a download>，在 WebView2/Tauri 中更可靠（Tauri 默认不处理 download 事件）。
  // 仅对携带 base64 data 的图片可用；asset URL 图片使用上方的下载按钮兜底。
  const handleSave = useCallback(async () => {
    if (!image.data) return;
    try {
      const selected = await save({
        defaultPath: downloadFilename(image),
        filters: [
          { name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif", "svg", "bmp"] },
        ],
      });
      if (!selected) return; // 用户取消
      await tauriApi.saveImageToFile(image.data, image.mimeType, selected);
      toast.success("图片已保存");
    } catch (error) {
      console.warn("[Lightbox] 保存图片失败:", error);
      toast.error("保存失败，请稍后再试");
    }
  }, [image]);

  // 拖拽平移
  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (scale <= 1) return;
      setIsDragging(true);
      dragStartRef.current = { x: e.clientX - pan.x, y: e.clientY - pan.y };
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
    },
    [scale, pan],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!isDragging) return;
      setPan({ x: e.clientX - dragStartRef.current.x, y: e.clientY - dragStartRef.current.y });
    },
    [isDragging],
  );

  const handlePointerUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  // 点击背景关闭
  const handleBackdropClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) onClose();
    },
    [onClose],
  );

  return (
    <motion.div
      className="lightbox-backdrop"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.2 }}
      onClick={handleBackdropClick}
    >
      <div className="lightbox-actions">
        <button className="lightbox-action-btn" onClick={() => zoomBy(ZOOM_STEP)} title="放大" aria-label="放大">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
            <circle cx="11" cy="11" r="6" /><path strokeLinecap="round" d="M11 8v6M8 11h6M20 20l-4.2-4.2" />
          </svg>
        </button>
        <button className="lightbox-action-btn" onClick={resetView} title="还原大小" aria-label="还原大小">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M3 12a9 9 0 1 0 3-6.7M3 4v5h5" />
          </svg>
        </button>
        <button className="lightbox-action-btn" onClick={() => void handleDownload()} title="下载图片" aria-label="下载图片">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 3v12m0 0 4-4m-4 4-4-4M5 21h14" />
          </svg>
        </button>
        {image.data && (
          <button className="lightbox-action-btn" onClick={() => void handleSave()} title="保存到…" aria-label="保存到本地">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M5 3h11l3 3v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Zm4 0v5h6V3M6 13h12v8H6v-8Z" />
            </svg>
          </button>
        )}
        <button className="lightbox-action-btn" onClick={onClose} title="关闭" aria-label="关闭">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      {/* 缩放指示器 */}
      {scale > 1 && (
        <div className="lightbox-zoom-indicator">{Math.round(scale * 100)}%</div>
      )}

      {/* 图片容器 */}
      <motion.div
        className={`lightbox-image-wrap ${scale > 1 ? "zoomed" : ""}`}
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onDoubleClick={handleDoubleClick}
        initial={{ scale: 0.92, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        exit={{ scale: 0.92, opacity: 0 }}
        transition={{ type: "spring", stiffness: 300, damping: 28 }}
      >
        <img
          src={source}
          alt={image.name || "图片预览"}
          style={{
            transform: `scale(${scale}) translate(${pan.x / scale}px, ${pan.y / scale}px)`,
            maxWidth: "90vw",
            maxHeight: "85vh",
            objectFit: "contain",
            transition: isDragging ? "none" : "transform 0.1s ease-out",
          }}
          draggable={false}
        />
      </motion.div>
    </motion.div>
  );
}

/** 全屏灯箱 —— 点击图片放大查看，滚轮缩放，拖拽平移 */
export function Lightbox({ image, onClose }: LightboxProps) {
  return (
    <AnimatePresence>
      {image && <LightboxInner key={image.id} image={image} onClose={onClose} />}
    </AnimatePresence>
  );
}
