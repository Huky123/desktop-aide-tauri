import { memo } from "react";

interface HeaderToolbarProps {
  isAiResponding: boolean;
  conversationTitle?: string;
  onClear: () => void;
  onSettings: () => void;
  onCollapse?: () => void;
  onToggleConversations?: () => void;
  onCapture?: () => void;
}

function ToolbarIcon({ name }: { name: "clear" | "settings" | "capture" }) {
  const common = {
    className: "w-3.5 h-3.5",
    fill: "none",
    viewBox: "0 0 24 24",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };

  if (name === "clear") {
    return (
      <svg {...common}>
        <path d="M3 6h18" />
        <path d="M8 6V4h8v2" />
        <path d="M9 11v6" />
        <path d="M15 11v6" />
        <path d="M6 6l1 14h10l1-14" />
      </svg>
    );
  }

  if (name === "capture") {
    return (
      <svg {...common}>
        <path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z" />
        <circle cx="12" cy="13" r="4" />
      </svg>
    );
  }

  if (name === "settings") {
    return (
      <svg {...common}>
        <path d="M12 15.5A3.5 3.5 0 1 0 12 8a3.5 3.5 0 0 0 0 7.5Z" />
        <path d="M19.4 15a1.8 1.8 0 0 0 .36 1.98l.04.04a2.1 2.1 0 0 1-2.98 2.98l-.04-.04A1.8 1.8 0 0 0 14.8 19.6a1.8 1.8 0 0 0-1.08 1.65V21.3a2.1 2.1 0 0 1-4.2 0v-.06A1.8 1.8 0 0 0 8.4 19.6a1.8 1.8 0 0 0-1.98.36l-.04.04a2.1 2.1 0 0 1-2.98-2.98l.04-.04A1.8 1.8 0 0 0 3.8 15a1.8 1.8 0 0 0-1.65-1.08H2.1a2.1 2.1 0 0 1 0-4.2h.06A1.8 1.8 0 0 0 3.8 8.6a1.8 1.8 0 0 0-.36-1.98l-.04-.04A2.1 2.1 0 0 1 6.38 3.6l.04.04A1.8 1.8 0 0 0 8.4 4a1.8 1.8 0 0 0 1.08-1.65V2.3a2.1 2.1 0 0 1 4.2 0v.06A1.8 1.8 0 0 0 14.8 4a1.8 1.8 0 0 0 1.98-.36l.04-.04a2.1 2.1 0 0 1 2.98 2.98l-.04.04A1.8 1.8 0 0 0 19.4 8.6a1.8 1.8 0 0 0 1.65 1.08h.06a2.1 2.1 0 0 1 0 4.2h-.06A1.8 1.8 0 0 0 19.4 15Z" />
      </svg>
    );
  }

  // fallback: chevron-up (used as collapse icon)
  return (
    <svg {...common}>
      <path d="m18 15-6-6-6 6" />
    </svg>
  );
}

export const HeaderToolbar = memo(function HeaderToolbar({
  isAiResponding,
  conversationTitle,
  onClear,
  onSettings,
  onCollapse,
  onToggleConversations,
  onCapture,
}: HeaderToolbarProps) {
  return (
    <div
      data-tauri-drag-region
      className="toolbar-shell flex items-center justify-between gap-2 px-2.5 py-2 cursor-grab active:cursor-grabbing"
    >
      {/* 左侧：对话切换 + 标题 */}
      <div className="flex min-w-0 items-center gap-1.5">
        {onToggleConversations && (
          <button
            onClick={onToggleConversations}
            className="btn-icon"
            title="对话列表"
            aria-label="对话列表"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              <path d="M8 9h8" /><path d="M8 13h6" />
            </svg>
          </button>
        )}
        <div className="min-w-0">
          <div className="toolbar-title" title={conversationTitle || "桌面助手"}>
            {conversationTitle || "桌面助手"}
          </div>
          {isAiResponding && (
            <div className="toolbar-status" aria-live="polite">
              <span className="toolbar-status-dot" />正在回复
            </div>
          )}
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1">
        {onCapture && (
          <button
            onClick={onCapture}
            disabled={isAiResponding}
            className="btn-icon"
            title="截图提问"
            aria-label="截图提问"
          >
            <ToolbarIcon name="capture" />
          </button>
        )}
        <button
          onClick={onClear}
          disabled={isAiResponding}
          className="btn-icon"
          title="清除对话"
          aria-label="清除对话"
        >
          <ToolbarIcon name="clear" />
        </button>
        <button onClick={onSettings} className="btn-icon" title="打开设置" aria-label="打开设置">
          <ToolbarIcon name="settings" />
        </button>
        {onCollapse && (
          <>
            <span
              className="w-px h-4 mx-0.5 rounded-full"
              style={{ background: "var(--border)" }}
              aria-hidden="true"
            />
            <button
              onClick={onCollapse}
              className="btn-collapse"
              title="收起面板"
              aria-label="收起面板"
            >
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round">
                <path d="m18 15-6-6-6 6" />
              </svg>
            </button>
          </>
        )}
      </div>
    </div>
  );
});
