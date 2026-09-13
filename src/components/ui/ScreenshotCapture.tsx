import { useCallback, useEffect, useRef, useState } from "react";
import type { CaptureScreenResult, ImageAttachment } from "../../types";

interface ScreenshotCaptureProps {
  data: CaptureScreenResult;
  onConfirm: (attachment: ImageAttachment) => void;
  onCancel: () => void;
}

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 截图选框遮罩 —— 显示全屏截图，拖拽选择区域，确认后裁剪为图片附件 */
export function ScreenshotCapture({ data, onConfirm, onCancel }: ScreenshotCaptureProps) {
  const imgRef = useRef<HTMLImageElement>(null);
  const [dragStart, setDragStart] = useState<{ x: number; y: number } | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);
  const [confirming, setConfirming] = useState(false);

  // Escape 取消
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onCancel]);

  // 拖拽选框（PointerEvent + setPointerCapture，与 Bubble 拖拽同模式）
  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    const img = imgRef.current;
    if (!img) return;
    const b = img.getBoundingClientRect();
    const x = Math.min(Math.max(e.clientX - b.left, 0), b.width);
    const y = Math.min(Math.max(e.clientY - b.top, 0), b.height);
    setDragStart({ x, y });
    setRect({ x, y, w: 0, h: 0 });
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }, []);

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!dragStart) return;
      const img = imgRef.current;
      if (!img) return;
      const b = img.getBoundingClientRect();
      const cx = Math.min(Math.max(e.clientX - b.left, 0), b.width);
      const cy = Math.min(Math.max(e.clientY - b.top, 0), b.height);
      setRect({
        x: Math.min(dragStart.x, cx),
        y: Math.min(dragStart.y, cy),
        w: Math.abs(cx - dragStart.x),
        h: Math.abs(cy - dragStart.y),
      });
    },
    [dragStart],
  );

  const handlePointerUp = useCallback(() => {
    setDragStart(null);
  }, []);

  // 确认：canvas 裁剪（CSS 坐标 → 物理像素）→ 生成 ImageAttachment
  const handleConfirm = useCallback(async () => {
    if (!rect || rect.w < 4 || rect.h < 4 || confirming) return;
    const img = imgRef.current;
    if (!img) return;
    setConfirming(true);
    try {
      const b = img.getBoundingClientRect();
      // 截图物理分辨率 / 显示尺寸 = 坐标缩放系数（与 DPR 无关）
      const scaleX = img.naturalWidth / b.width;
      const scaleY = img.naturalHeight / b.height;
      const sx = Math.round(rect.x * scaleX);
      const sy = Math.round(rect.y * scaleY);
      const sw = Math.round(rect.w * scaleX);
      const sh = Math.round(rect.h * scaleY);

      const canvas = document.createElement("canvas");
      canvas.width = sw;
      canvas.height = sh;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("无法创建画布上下文");
      ctx.drawImage(img, sx, sy, sw, sh, 0, 0, sw, sh);

      const dataUrl = canvas.toDataURL("image/png");
      const comma = dataUrl.indexOf(",");
      const b64 = comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
      const stamp = new Date()
        .toLocaleTimeString("zh-CN", { hour12: false })
        .replace(/:/g, "-");

      onConfirm({
        type: "image",
        id: crypto.randomUUID(),
        data: b64,
        mimeType: "image/png",
        name: `截图-${stamp}.png`,
        size: Math.round(b64.length * 0.75),
        width: sw,
        height: sh,
        source: "screenshot",
      });
    } catch (err) {
      console.warn("[ScreenshotCapture] 裁剪失败:", err);
    } finally {
      setConfirming(false);
    }
  }, [rect, confirming, onConfirm]);

  const hasSelection = !!rect && rect.w >= 4 && rect.h >= 4;

  return (
    <div className="fixed inset-0 z-[100] overflow-hidden" style={{ cursor: "crosshair" }}>
      {/* 全屏截图 */}
      <img
        ref={imgRef}
        src={`data:image/png;base64,${data.image_base64}`}
        alt="屏幕截图"
        draggable={false}
        className="w-full h-full select-none"
        style={{ objectFit: "fill", display: "block" }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
      />

      {/* 选框 + 外部压暗 */}
      {hasSelection && (
        <div
          className="absolute border-2"
          style={{
            left: rect.x,
            top: rect.y,
            width: rect.w,
            height: rect.h,
            borderColor: "var(--accent)",
            background: "rgba(129,140,248,0.12)",
            boxShadow: "0 0 0 9999px rgba(0,0,0,0.45)",
            pointerEvents: "none",
          }}
        />
      )}

      {/* 工具栏 */}
      <div
        className="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center gap-2 px-3 py-2 rounded-xl select-none"
        style={{
          background: "rgba(18,18,22,0.92)",
          border: "1px solid var(--border)",
          boxShadow: "0 8px 32px rgba(0,0,0,0.4)",
        }}
      >
        <span className="text-[11px] px-1 tabular-nums" style={{ color: "var(--text-tertiary)" }}>
          {hasSelection
            ? `${Math.round(rect.w)} × ${Math.round(rect.h)}`
            : "拖拽选择要提问的区域"}
        </span>
        <button
          onClick={() => void handleConfirm()}
          disabled={!hasSelection || confirming}
          className="px-3 py-1 rounded-md text-[11px] font-semibold transition-opacity disabled:opacity-40"
          style={{ background: "var(--accent)", color: "#fff" }}
        >
          {confirming ? "处理中…" : "发送"}
        </button>
        <button
          onClick={onCancel}
          className="px-3 py-1 rounded-md text-[11px] font-medium transition-colors"
          style={{ background: "var(--surface-raised)", color: "var(--text-secondary)" }}
        >
          取消
        </button>
      </div>
    </div>
  );
}
