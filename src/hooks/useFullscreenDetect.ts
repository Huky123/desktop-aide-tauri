import { useEffect, useRef } from "react";
import { useUiStore } from "../store/uiStore";
import { useConfigStore } from "../store/configStore";
import { tauriApi } from "../services/tauriApi";

/**
 * 检测前台窗口是否最大化 + 气泡自动折叠定时器。
 *
 * 1. 前台最大化时气泡立即收折为边缘细条（最高优先级）。
 * 2. 如果启用了 bubble_auto_collapse，面板关闭后闲置
 *    bubble_collapse_delay 秒自动收折。一旦收折即保持，
 *    直到面板重新打开才会展开。
 */
export function useFullscreenDetect() {
  const isPanelOpen = useUiStore((s) => s.isPanelOpen);
  const setBubbleCollapsed = useUiStore((s) => s.setBubbleCollapsed);
  const bubbleAutoCollapse = useConfigStore((s) => s.config.bubble_auto_collapse);
  const bubbleCollapseDelay = useConfigStore((s) => s.config.bubble_collapse_delay);
  const isPanelOpenRef = useRef(isPanelOpen);

  useEffect(() => {
    isPanelOpenRef.current = isPanelOpen;
  }, [isPanelOpen]);

  // 自动折叠定时器引用
  const autoCollapseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // 追踪自动折叠是否已触发（防止全屏轮询把气泡重新展开）
  const autoCollapsedRef = useRef(false);

  useEffect(() => {
    // 面板打开时气泡始终完整可见
    if (isPanelOpen) {
      setBubbleCollapsed(false);
      autoCollapsedRef.current = false;
      if (autoCollapseTimerRef.current) {
        clearTimeout(autoCollapseTimerRef.current);
        autoCollapseTimerRef.current = null;
      }
      return;
    }

    let cancelled = false;
    let seq = 0;

    const check = async () => {
      const currentSeq = ++seq;
      try {
        const fullscreen = await tauriApi.isForegroundFullscreen();
        if (cancelled || currentSeq !== seq || isPanelOpenRef.current) return;

        if (fullscreen) {
          // 前台最大化 → 立即收折
          setBubbleCollapsed(true);
          return;
        }

        // 非全屏状态
        if (bubbleAutoCollapse) {
          // 自动折叠已触发 → 保持收折状态，不展开
          if (autoCollapsedRef.current) return;

          // 首次进入非全屏 → 启动定时器
          if (!autoCollapseTimerRef.current) {
            autoCollapseTimerRef.current = setTimeout(() => {
              if (!cancelled && !isPanelOpenRef.current) {
                autoCollapsedRef.current = true;
                setBubbleCollapsed(true);
              }
              autoCollapseTimerRef.current = null;
            }, bubbleCollapseDelay * 1000);
          }
          // 定时器运行中 → 保持完整气泡
          setBubbleCollapsed(false);
        } else {
          // 自动折叠未启用 → 保持完整气泡
          setBubbleCollapsed(false);
        }
      } catch {
        if (!cancelled && currentSeq === seq) {
          setBubbleCollapsed(false);
        }
      }
    };

    check();
    const timer = setInterval(check, 3000);

    return () => {
      cancelled = true;
      clearInterval(timer);
      if (autoCollapseTimerRef.current) {
        clearTimeout(autoCollapseTimerRef.current);
        autoCollapseTimerRef.current = null;
      }
    };
  }, [isPanelOpen, setBubbleCollapsed, bubbleAutoCollapse, bubbleCollapseDelay]);
}
