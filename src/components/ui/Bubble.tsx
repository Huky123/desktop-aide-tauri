import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useUiStore } from "../../store/uiStore";
import { useSessionStore } from "../../store/sessionStore";
import { useReminderStore, selectBadgeCount } from "../../store/reminderStore";
import { tauriApi } from "../../services/tauriApi";

/** 提醒脉冲动画总时长（毫秒），与 bubble-reminder-pulse 关键帧保持一致 */
const PULSE_MS = 2500;

export function Bubble() {
  const isPanelOpen = useUiStore((s) => s.isPanelOpen);
  const isAiResponding = useSessionStore((s) => s.isAiResponding);
  const bubbleCollapsed = useUiStore((s) => s.bubbleCollapsed);

  // 提醒未读徽标计数 + 脉冲触发器（每次提醒触发 pulseCount+1）
  const badgeCount = useReminderStore(selectBadgeCount);
  const pulseCount = useReminderStore((s) => s.pulseCount);
  /**
   * 已经播完脉冲动画的那个 `pulseCount` 值。
   *
   * 用"派生 + 记录已结算值"代替"在 effect 里同步 setState(false)"：
   * 后者会触发级联渲染（react-hooks/set-state-in-effect）。两者语义等价——
   * 初始值 0 与 `pulseCount` 初值 0 一致，因此首屏不会误判为"正在脉冲"。
   */
  const [settledPulseCount, setSettledPulseCount] = useState(0);

  // 脉冲动画结束后标记为已结算，恢复原有的呼吸/静止动画
  useEffect(() => {
    if (pulseCount === 0) return;
    const timer = setTimeout(() => setSettledPulseCount(pulseCount), PULSE_MS);
    return () => clearTimeout(timer);
  }, [pulseCount]);

  const pulsing = pulseCount > 0 && pulseCount !== settledPulseCount;

  // 防抖：防止双击/快速连点导致面板反复开关
  const lastToggleRef = useRef(0);
  const TOGGLE_COOLDOWN = 400;

  // 鼠标悬停时临时展开 strip
  const [hovered, setHovered] = useState(false);

  // 面板已打开时永远不显示 strip；hover 时展开完整气泡
  const isStrip = bubbleCollapsed && !hovered && !isPanelOpen;

  const handleContextMenu = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    void tauriApi.showBubbleMenu();
  }, []);

  const togglePanel = useCallback(() => {
    const now = Date.now();
    if (now - lastToggleRef.current < TOGGLE_COOLDOWN) return;
    lastToggleRef.current = now;
    // 用户亲手打开面板 → 未读提醒视为已读（方案 A：「点开面板后消失」）
    if (!useUiStore.getState().isPanelOpen) {
      useReminderStore.getState().markAllRead();
    }
    useUiStore.getState().togglePanel();
  }, []);

  // 拖拽与点击判定统一挂在容器上，内部视觉元素可安全重挂载（脉冲动画用 key 重启）
  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0) return;

      const startX = e.screenX;
      const startY = e.screenY;
      const el = e.currentTarget as HTMLElement;

      // 夺取指针捕获，确保即使指针移出元素或原生拖拽介入，仍能收到 pointerup
      el.setPointerCapture(e.pointerId);

      let dragStarted = false;

      const onPointerMove = (ev: PointerEvent) => {
        if (dragStarted) return;
        const dx = Math.abs(ev.screenX - startX);
        const dy = Math.abs(ev.screenY - startY);
        // 超过 4px → 确认为拖拽，启动原生窗口拖拽
        if (dx > 4 || dy > 4) {
          dragStarted = true;
          el.style.cursor = "grabbing";
          cleanup();
          getCurrentWindow().startDragging().catch(() => {});
        }
      };

      const onPointerUp = () => {
        cleanup();
        if (!dragStarted) {
          togglePanel();
        }
      };

      const cleanup = () => {
        el.releasePointerCapture(e.pointerId);
        el.removeEventListener("pointermove", onPointerMove);
        el.removeEventListener("pointerup", onPointerUp);
        el.style.cursor = "";
      };

      el.addEventListener("pointermove", onPointerMove);
      el.addEventListener("pointerup", onPointerUp);
    },
    [togglePanel],
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      togglePanel();
    }
  };

  return (
    <div
      className="fixed z-20 select-none"
      tabIndex={0}
      role="button"
      aria-label={isPanelOpen ? "收起面板" : "展开助手面板"}
      aria-expanded={isPanelOpen}
      onKeyDown={handleKeyDown}
      onContextMenu={handleContextMenu}
      onPointerDown={handlePointerDown}
      style={{
        // ── 位置与尺寸：strip / normal 两态（面板打开时不渲染内容）───
        right: isStrip ? 0 : 8,
        top: isStrip ? "35%" : 8,
        width: isStrip ? 24 : 40,
        height: isStrip ? 48 : 40,
        outline: "none",
        transition:
          "right 0.25s ease, top 0.25s ease, width 0.25s ease, height 0.25s ease, border-radius 0.25s ease",
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* ── Strip 形态：屏幕右边缘半透明细条（4px 可见 + 20px 隐形命中区）─── */}
      {isStrip && (
        <div
          className="h-full rounded-l-md"
          style={{
            background: "var(--accent)",
            opacity: 0.25,
            width: 4,
            marginLeft: "auto",
            cursor: "pointer",
          }}
          aria-label="展开助手面板"
        />
      )}

      {/* ── 完整气泡形态（面板关闭时）─── */}
      {!isPanelOpen && !isStrip && (
        <div
          // 提醒触发时用 pulseCount 作 key 重挂载 → 脉冲动画重新播放
          key={pulsing ? `pulse-${pulseCount}` : "bubble"}
          className="relative w-10 h-10 rounded-[10px] flex items-center justify-center backdrop-blur-lg transition-all duration-150 hover:scale-[1.02] active:scale-[0.97]"
          style={{
            background: hovered ? "var(--bubble-bg-hover)" : "var(--bubble-bg)",
            border: "1px solid var(--bubble-border)",
            boxShadow: hovered ? "var(--bubble-shadow-hover)" : "var(--bubble-shadow)",
            cursor: "grab",
            animation: pulsing
              ? "bubble-reminder-pulse 2.4s ease-in-out 1"
              : bubbleCollapsed && hovered
                ? "bubble-peek 0.25s ease-out"
                : isAiResponding
                  ? "bubble-breathe 2.5s ease-in-out infinite"
                  : "none",
          }}
        >
          <svg
            className="w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
            aria-hidden="true"
            style={{
              color: isAiResponding
                ? "var(--accent)"
                : "var(--bubble-icon-idle)",
              pointerEvents: "none",
            }}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z"
            />
          </svg>

          {/* 未读提醒徽标：点开面板后消失 */}
          {badgeCount > 0 && (
            <span
              key={`badge-${badgeCount}`}
              className="bubble-badge"
              aria-hidden="true"
            >
              {badgeCount > 9 ? "9+" : badgeCount}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
