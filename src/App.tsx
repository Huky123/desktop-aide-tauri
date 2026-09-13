import { useCallback } from "react";
import { Toaster } from "sonner";
import { Bubble } from "./components/ui/Bubble";
import { ChatPanel } from "./components/chat/ChatPanel";
import { useAppConfiguration } from "./hooks/useAppConfiguration";
import { usePanelWindowSync } from "./hooks/usePanelWindowSync";
import { useFullscreenDetect } from "./hooks/useFullscreenDetect";
import { useTauriEvent } from "./hooks/useTauriEvent";
import { useReminderEvents } from "./hooks/useReminderEvents";
import { useReminderStore } from "./store/reminderStore";
import { tauriApi } from "./services/tauriApi";
import { useSessionStore } from "./store/sessionStore";
import { useUiStore } from "./store/uiStore";

function App() {
  const isPanelOpen = useUiStore((state) => state.isPanelOpen);
  const exitConfirmOpen = useUiStore((state) => state.exitConfirmOpen);

  useFullscreenDetect();
  useAppConfiguration();
  usePanelWindowSync(isPanelOpen);
  // 提醒触发：气泡脉冲 + 未读徽标 + 面板内助手消息（方案 A）
  useReminderEvents();

  const requestExit = useCallback(() => {
    if (useSessionStore.getState().isAiResponding) {
      useUiStore.getState().setExitConfirmOpen(true);
      return true;
    }

    void tauriApi.quitApp();
    return false;
  }, []);

  useTauriEvent("tray-open-panel", () => {
    // 用户亲手打开面板 → 未读提醒视为已读（方案 A：「点开面板后消失」）
    useReminderStore.getState().markAllRead();
    useUiStore.getState().setPanelOpen(true);
  });
  useTauriEvent("app-exit-requested", requestExit);

  // 全局热键：Ctrl+Alt+Space 切换面板
  useTauriEvent("global-toggle-panel", () => {
    if (!useUiStore.getState().isPanelOpen) {
      useReminderStore.getState().markAllRead();
    }
    useUiStore.getState().togglePanel();
  });

  // 全局热键：Ctrl+Alt+C 截图提问（先确保面板打开）
  useTauriEvent("capture-hotkey", () => {
    useReminderStore.getState().markAllRead();
    useUiStore.getState().setPanelOpen(true);
    void tauriApi.captureScreen().catch((err) => {
      console.warn("[App] 热键截图失败:", err);
    });
  });

  const cancelExit = useCallback(() => {
    useUiStore.getState().setExitConfirmOpen(false);
    void tauriApi.togglePanel(useUiStore.getState().isPanelOpen);
  }, []);

  return (
    <div
      className="relative w-screen h-screen overflow-hidden"
      style={{
        pointerEvents: isPanelOpen ? "auto" : "none",
        background: "transparent",
        backdropFilter: "none",
      }}
    >
      <div style={{ pointerEvents: isPanelOpen ? "none" : "auto" }}>
        <Bubble />
      </div>

      <div
        style={{
          pointerEvents: isPanelOpen ? "auto" : "none",
          visibility: isPanelOpen ? "visible" : "hidden",
        }}
      >
        <ChatPanel />
      </div>

      {exitConfirmOpen && (
        <div className="app-confirm-backdrop" role="presentation">
          <div
            className="app-confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="exit-confirm-title"
          >
            <div className="app-confirm-icon" aria-hidden="true">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 9v4" />
                <path d="M12 17h.01" />
                <path d="M10.3 3.7 2.6 17a2 2 0 0 0 1.7 3h15.4a2 2 0 0 0 1.7-3L13.7 3.7a2 2 0 0 0-3.4 0Z" />
              </svg>
            </div>
            <h2 id="exit-confirm-title">退出桌面助手？</h2>
            <p>AI 正在回复，退出会中断当前任务。</p>
            <div className="app-confirm-actions">
              <button type="button" onClick={cancelExit}>继续等待</button>
              <button type="button" className="danger" onClick={() => void tauriApi.quitApp()}>
                退出应用
              </button>
            </div>
          </div>
        </div>
      )}

      <Toaster
        position="top-center"
        duration={2000}
        toastOptions={{
          style: {
            background: "var(--surface-raised)",
            color: "var(--text-primary)",
            border: "1px solid var(--border)",
            fontSize: "12px",
            borderRadius: "10px",
            padding: "8px 14px",
            fontWeight: 500,
          },
        }}
      />
    </div>
  );
}

export default App;
