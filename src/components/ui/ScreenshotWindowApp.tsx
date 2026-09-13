import { useEffect, useState } from "react";
import { toast } from "sonner";
import { useAppConfiguration } from "../../hooks/useAppConfiguration";
import { tauriApi } from "../../services/tauriApi";
import type { CaptureScreenResult, ImageAttachment } from "../../types";
import { ScreenshotCapture } from "./ScreenshotCapture";

/**
 * 独立截图窗口入口：拉取后端暂存的全屏截图，渲染选框 UI。
 * 确认 → 提交裁剪附件（后端广播给主窗口并关闭本窗口）；
 * 取消 → 通知后端清理并关闭。
 */
export default function ScreenshotWindowApp() {
  // 独立 WebView 无主题变量，注入应用主题（accent/border 等）
  useAppConfiguration();

  const [data, setData] = useState<CaptureScreenResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    tauriApi
      .getScreenshotData()
      .then(setData)
      .catch((e) => {
        console.warn("[ScreenshotWindow] 获取截图失败:", e);
        setError(String(e));
      });
  }, []);

  const handleConfirm = async (att: ImageAttachment) => {
    try {
      await tauriApi.submitScreenshotCapture({
        data: att.data,
        mimeType: att.mimeType,
        name: att.name,
        width: att.width ?? 0,
        height: att.height ?? 0,
      });
      // 窗口由后端在提交成功后关闭
    } catch (e) {
      console.warn("[ScreenshotWindow] 提交失败:", e);
      toast.error("发送失败，请重试");
    }
  };

  const handleCancel = () => {
    void tauriApi.cancelScreenshot().catch(console.warn);
  };

  if (error) {
    return (
      <div
        className="fixed inset-0 flex items-center justify-center"
        style={{ background: "#0d0d11" }}
      >
        <div className="text-center space-y-3 px-6">
          <p className="text-sm font-medium" style={{ color: "var(--text-primary)" }}>
            无法获取截图
          </p>
          <p className="text-xs break-all" style={{ color: "var(--text-tertiary)" }}>
            {error}
          </p>
          <button
            onClick={handleCancel}
            className="px-3 py-1.5 rounded-md text-xs font-medium"
            style={{ background: "var(--surface-raised)", color: "var(--text-secondary)" }}
          >
            关闭
          </button>
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div
        className="fixed inset-0 flex items-center justify-center"
        style={{ background: "#0d0d11" }}
      >
        <span className="text-xs" style={{ color: "var(--text-tertiary)" }}>
          正在获取截图…
        </span>
      </div>
    );
  }

  return (
    <ScreenshotCapture
      data={data}
      onConfirm={(att) => void handleConfirm(att)}
      onCancel={handleCancel}
    />
  );
}
